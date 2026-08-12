// Add / Import screen: Steam import only (fetches and parses the account's
// real market history via live invoke() commands).

import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { state, notify } from "../state.js";
import { ensureSettingsLoaded } from "../items-store.js";
import * as api from "../api.js";

const STEAM_MARKET_URL = "https://steamcommunity.com/market/";
const RUST_APPID = 252490;

// main.js's re-render restores input focus by element `id` only (see its
// comment on focus/selection restore) — a class + data-row-id alone isn't
// enough, so every keystroke into a per-row input would blur it. row_id can
// contain characters that aren't valid in a bare HTML id (spaces, colons),
// hence the sanitize.
function steamRowInputId(prefix, rowId) {
  return `${prefix}-${rowId.replace(/[^a-zA-Z0-9_-]/g, "_")}`;
}

function fromHTML(html) {
  // Returns the full content fragment, matching portfolio.js's convention —
  // this screen's markup is several sibling top-level divs, not one wrapper.
  const template = document.createElement("template");
  template.innerHTML = html.trim();
  return template.content;
}

function reloadItemsAndGoToPortfolio() {
  state.items = undefined;
  state.screen = "portfolio";
  notify();
}

function steamImportTabHTML() {
  if (!state.hasSteamCookie) {
    return `
      <div style="max-width:480px; padding:16px; border:1px solid #23282d; border-radius:5px; background:#101315;">
        <div style="font-size:15px; color:#c9d0d5;">No Steam account connected.</div>
        <div style="font-size:14px; color:#7d868f; margin-top:6px; line-height:1.6;">Add your Steam session cookie in Settings to fetch and import your real market history.</div>
      </div>
    `;
  }

  const filterRust = state.steamImportFilter === "rust";
  const bought = state.steamImportRows.filter(
    (r) => r.action === "Bought" && (filterRust ? r.appid === RUST_APPID : true),
  );
  const sold = state.steamImportRows.filter(
    (r) => r.action === "Sold" && (filterRust ? r.appid === RUST_APPID : true),
  );
  // Store-purchase price fills (existing items, price/date updated on
  // commit) and flagged pack purchases (informational only — see
  // commands::preview_steam_import). Both empty when no store cookie is
  // saved.
  const priceFills = state.steamPriceFills.filter((f) => (filterRust ? f.appid === RUST_APPID : true));
  const flaggedPacks = state.steamFlaggedPacks.filter((p) => (filterRust ? p.appid === RUST_APPID : true));
  const includedCount = bought.filter((r) => r.included).length;
  const hasAnyData = state.steamImportRows.length > 0 || state.steamPriceFills.length > 0 || state.steamFlaggedPacks.length > 0;
  const commitEnabled = includedCount > 0 || priceFills.length > 0;

  return `
    <div style="display:flex; flex-direction:column; gap:14px; height:100%; min-height:0;">
      <div style="display:flex; align-items:center; gap:10px; flex:none;">
        <div id="sl-steam-fetch" style="padding:8px 16px; background:#d98a52; color:#0c0e10; border-radius:4px; font-size:15px; font-weight:600; cursor:pointer; ${state.steamImportLoading ? "opacity:0.6; pointer-events:none;" : ""}">${state.steamImportLoading ? "Fetching…" : "Fetch from Steam"}</div>
        <div style="display:flex; gap:6px;">
          <div class="sl-steam-filter" data-filter="rust" style="padding:6px 12px; border:1px solid ${filterRust ? "#d98a52" : "#2b3137"}; background:${filterRust ? "#241a10" : "#15181b"}; color:${filterRust ? "#d98a52" : "#b9c1c7"}; border-radius:4px; font-size:14px; cursor:pointer;">Rust</div>
          <div class="sl-steam-filter" data-filter="all" style="padding:6px 12px; border:1px solid ${!filterRust ? "#d98a52" : "#2b3137"}; background:${!filterRust ? "#241a10" : "#15181b"}; color:${!filterRust ? "#d98a52" : "#b9c1c7"}; border-radius:4px; font-size:14px; cursor:pointer;">All games</div>
        </div>
      </div>

      ${
        state.steamImportLog.length
          ? `<div id="sl-steam-log" data-scroll-key="steam-log" style="flex:none; max-height:130px; overflow-y:auto; border:1px solid #2b3137; border-radius:4px; background:#101315; padding:8px 10px; font-family:'IBM Plex Mono', monospace; font-size:12.5px; color:#7d868f; line-height:1.7;">
              ${state.steamImportLog.map((line) => `<div>${line}</div>`).join("")}
            </div>`
          : ""
      }

      ${
        !hasAnyData
          ? `<div style="font-size:14px; color:#7d868f;">Fetching walks your entire market history on first run — this can take a while for a large account. Repeat imports only fetch new activity.</div>`
          : `
        ${
          bought.length
            ? `<div data-scroll-key="steam-table" style="flex:1; overflow-y:auto; min-height:0; border:1px solid #2b3137; border-radius:4px; background:#101315;">
          <div style="display:grid; grid-template-columns:20px minmax(0,1fr) 130px 78px 100px; padding:0 10px; height:26px; align-items:center; border-bottom:1px solid #23282d; font-family:'IBM Plex Mono', monospace; font-size:11.5px; letter-spacing:0.1em; color:#5d666d; position:sticky; top:0; background:#101315;">
            <div></div><div>ITEM</div><div>GAME</div><div style="text-align:right;">PAID</div><div style="text-align:right;">DATE</div>
          </div>
          ${bought
            .map((r) => {
              const unknownPrice = r.price == null;
              const unknownDate = !r.raw_date;
              return `
            <div class="sl-steam-row" data-row-id="${r.row_id}" style="display:grid; grid-template-columns:20px minmax(0,1fr) 130px 78px 100px; padding:0 10px; height:34px; align-items:center; border-bottom:1px solid #191d20; white-space:nowrap; font-size:14.5px;">
              <input type="checkbox" class="sl-steam-row-included" data-row-id="${r.row_id}" ${r.included ? "checked" : ""} style="cursor:pointer;" />
              <div style="overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:#e3e7ea;" title="${unknownPrice ? "Currently held but no matching Steam Market purchase was found (gifted, traded, or crafted) — price and date are unknown, please fill them in." : ""}">${r.market_hash_name}${unknownPrice ? ' <span style="color:#d98a52;">?</span>' : ""}</div>
              <div style="overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:#7d868f; font-size:13px;">${r.game_name || "—"}</div>
              <input type="number" step="0.01" min="0" id="${steamRowInputId("sl-steam-price", r.row_id)}" class="sl-steam-row-price" data-row-id="${r.row_id}" value="${r.price ?? ""}" placeholder="${unknownPrice ? "n/a" : ""}" style="text-align:right; background:transparent; border:none; border-bottom:1px solid ${unknownPrice ? "#d98a52" : "transparent"}; color:${unknownPrice ? "#d98a52" : "#c9d0d5"}; font-family:'IBM Plex Mono', monospace; font-size:13px; width:70px;" />
              ${
                unknownDate
                  ? `<div style="text-align:right; color:#d98a52; font-family:'IBM Plex Mono', monospace; font-size:13px;">n/a</div>`
                  : `<input type="date" id="${steamRowInputId("sl-steam-date", r.row_id)}" class="sl-steam-row-date" data-row-id="${r.row_id}" value="${r.raw_date}" style="text-align:right; background:transparent; border:none; border-bottom:1px solid transparent; color:#7d868f; font-family:'IBM Plex Mono', monospace; font-size:13px; width:100px;" />`
              }
            </div>`;
            })
            .join("")}
        </div>`
            : ""
        }
        ${
          sold.length
            ? `<div style="flex:none; font-size:13px; color:#5d666d;">${sold.length} sale${sold.length === 1 ? "" : "s"} found in this history — informational only, not imported (this app doesn't reduce holdings from market sales yet).</div>`
            : ""
        }
        ${
          priceFills.length
            ? `<div style="flex:none;">
          <div style="font-family:'IBM Plex Mono', monospace; font-size:11.5px; letter-spacing:0.1em; color:#5d666d; margin-bottom:6px;">STORE PRICE FILLS (${priceFills.length})</div>
          <div data-scroll-key="price-fills" style="max-height:150px; overflow-y:auto; border:1px solid #2b3137; border-radius:4px; background:#101315;">
            ${priceFills
              .map(
                (f) => `
            <div style="display:grid; grid-template-columns:minmax(0,1fr) 90px 100px; padding:0 10px; height:30px; align-items:center; border-bottom:1px solid #191d20; white-space:nowrap; font-size:13.5px;">
              <div style="overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:#e3e7ea;">${f.item_name}</div>
              <div style="text-align:right; font-family:'IBM Plex Mono', monospace; font-size:13px; color:#69c98a;">${f.price.toFixed(2)}</div>
              <div style="text-align:right; font-family:'IBM Plex Mono', monospace; font-size:13px; color:#7d868f;">${f.date}</div>
            </div>`,
              )
              .join("")}
          </div>
          <div style="font-size:12.5px; color:#5d666d; margin-top:4px; line-height:1.5;">Matched from your Steam store purchase history to items already in your vault with an unknown price — applied automatically on commit.</div>
        </div>`
            : ""
        }
        ${
          flaggedPacks.length
            ? `<div style="flex:none;">
          <div style="font-family:'IBM Plex Mono', monospace; font-size:11.5px; letter-spacing:0.1em; color:#5d666d; margin-bottom:6px;">FLAGGED PACK PURCHASES (${flaggedPacks.length})</div>
          <div data-scroll-key="flagged-packs" style="max-height:150px; overflow-y:auto; border:1px solid #2b3137; border-radius:4px; background:#101315;">
            ${flaggedPacks
              .map(
                (p) => `
            <div style="padding:6px 10px; border-bottom:1px solid #191d20; font-size:13.5px;">
              <div style="color:#e3e7ea;">${p.item_names.join(", ")}</div>
              <div style="color:#d98a52; font-size:12.5px; margin-top:2px;">${p.total_price != null ? `${p.currency}${p.total_price.toFixed(2)}` : "price unknown"} on ${p.date} — could not match to a specific item, add manually if needed.</div>
            </div>`,
              )
              .join("")}
          </div>
        </div>`
            : ""
        }
        <div style="display:flex; gap:8px; flex:none;">
          <div id="sl-steam-commit" style="padding:8px 16px; background:#d98a52; color:#0c0e10; border-radius:4px; font-size:15px; font-weight:600; cursor:pointer; ${commitEnabled ? "" : "opacity:0.5; pointer-events:none;"}">Commit ${includedCount} item${includedCount === 1 ? "" : "s"}${priceFills.length ? ` + ${priceFills.length} price fill${priceFills.length === 1 ? "" : "s"}` : ""}</div>
        </div>
      `
      }
    </div>
  `;
}

export function render() {
  ensureSettingsLoaded();

  // The user may open this screen without ever having visited Settings
  // first, so this can't rely on settings.js's own fetch of the same flag
  // having already run.
  if (state.hasSteamCookie === undefined) {
    api.hasSteamCookie().then((v) => {
      state.hasSteamCookie = v;
      notify();
    });
  }

  const root = fromHTML(`
    <div style="flex:none; padding:16px 20px 0;">
      <div style="font-size:20.5px; font-weight:600; letter-spacing:-0.01em;">Add / Import</div>
      <div style="font-size:14px; color:#7d868f; margin-top:2px;">Import your real <span id="sl-steam-market-link" style="color:#d98a52; cursor:pointer; text-decoration:underline;">Steam market history</span> directly.</div>
    </div>

    <div data-scroll-key="add-content" style="flex:1; overflow-y:auto; min-height:0; padding:18px 20px;">
      ${steamImportTabHTML()}
    </div>
  `);

  root.querySelector("#sl-steam-market-link").addEventListener("click", () => {
    openUrl(STEAM_MARKET_URL).catch((err) => console.error("Failed to open Steam market URL", err));
  });

  if (state.hasSteamCookie) {
    // Deferred to the next frame and queried against the live document, not
    // `root` — at this point `root` is still a detached DocumentFragment
    // (main.js hasn't appendChild'ed it into #screen-root yet), and
    // scrollHeight on a detached, not-yet-laid-out element isn't reliable.
    requestAnimationFrame(() => {
      const logEl = document.getElementById("sl-steam-log");
      if (logEl) logEl.scrollTop = logEl.scrollHeight;
    });

    root.querySelector("#sl-steam-fetch").addEventListener("click", () => {
      state.steamImportLoading = true;
      state.steamImportLog = [];
      notify();

      // A full history + multi-game inventory sync can easily take tens of
      // seconds; without this, the button just sits on "Fetching…" with no
      // sign of life. The backend emits one event per meaningful step (see
      // commands::preview_steam_import).
      listen("steam-import-progress", (event) => {
        state.steamImportLog = [...state.steamImportLog, event.payload];
        notify();
      }).then((unlisten) => {
        api
          .previewSteamImport()
          .then((preview) => {
            unlisten();
            state.steamImportRows = preview.transactions.map((r) => ({ ...r, included: true }));
            state.steamPriceFills = preview.price_fills;
            state.steamFlaggedPacks = preview.flagged_packs;
            state.steamImportLoading = false;
            notify();
          })
          .catch((err) => {
            unlisten();
            console.error("Failed to fetch Steam import", err);
            // Without this, a failure (e.g. an expired Steam cookie) looks
            // identical to a silent hang — the log just stops mid-sentence
            // with no indication anything went wrong, since console.error
            // is invisible in a packaged release build with no devtools.
            state.steamImportLog = [...state.steamImportLog, `Error: ${err}`];
            state.steamImportLoading = false;
            notify();
          });
      });
    });

    root.querySelectorAll(".sl-steam-filter").forEach((el) => {
      el.addEventListener("click", () => {
        state.steamImportFilter = el.dataset.filter;
        notify();
      });
    });

    root.querySelectorAll(".sl-steam-row-included").forEach((el) => {
      el.addEventListener("change", (e) => {
        const row = state.steamImportRows.find((r) => r.row_id === el.dataset.rowId);
        if (row) row.included = e.target.checked;
        notify();
      });
    });

    root.querySelectorAll(".sl-steam-row-date").forEach((el) => {
      el.addEventListener("input", (e) => {
        const row = state.steamImportRows.find((r) => r.row_id === el.dataset.rowId);
        if (row) row.raw_date = e.target.value;
        notify();
      });
    });

    root.querySelectorAll(".sl-steam-row-price").forEach((el) => {
      el.addEventListener("input", (e) => {
        const row = state.steamImportRows.find((r) => r.row_id === el.dataset.rowId);
        if (row) {
          const value = parseFloat(e.target.value);
          row.price = Number.isFinite(value) ? value : null;
        }
        notify();
      });
    });

    const commitButton = root.querySelector("#sl-steam-commit");
    if (commitButton) {
      commitButton.addEventListener("click", () => {
        // Must re-apply the SAME appid filter used to decide what's shown
        // in the table — every row defaults to included: true regardless of
        // the filter (only visible rows can be unchecked by the user), so
        // without this, hidden rows from other games get committed too.
        // Price fills have no per-row checkbox, so the game filter is the
        // only way to scope them the same way.
        const filterRust = state.steamImportFilter === "rust";
        const included = state.steamImportRows.filter(
          (r) => r.action === "Bought" && r.included && (filterRust ? r.appid === RUST_APPID : true),
        );
        const priceFills = state.steamPriceFills.filter((f) => (filterRust ? f.appid === RUST_APPID : true));
        api
          .commitSteamImport(included, priceFills)
          .then(() => {
            state.steamImportRows = [];
            state.steamPriceFills = [];
            state.steamFlaggedPacks = [];
            reloadItemsAndGoToPortfolio();
          })
          .catch((err) => console.error("Failed to commit Steam import", err));
      });
    }
  }

  return root;
}
