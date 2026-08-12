// Portfolio screen: header, stat tiles, search/filter bar, sortable column
// headers, and the item table — all wired to live data via src/api.js.

import { state, notify } from "../state.js";
import { ensureGameNamesBackfilled, ensureItemsLoaded, ensureSettingsLoaded } from "../items-store.js";
import * as api from "../api.js";
import { col, deriveRows, deriveTotals, gameOf, gamesOf, money, smoney, timeAgo } from "./portfolio-derive.js";
import { symbolForCode } from "../currency.js";
import { listen } from "@tauri-apps/api/event";

function fromHTML(html) {
  // Returns the full content fragment (not just firstElementChild) because
  // this screen's markup is several sibling top-level divs, not one wrapper
  // — a fragment preserves all of them when appended into #screen-root.
  const template = document.createElement("template");
  template.innerHTML = html.trim();
  return template.content;
}

const COLUMNS = "30px minmax(90px,1fr) 62px 36px 80px 80px 70px 90px 70px 62px 26px";

const SORT_LABELS = {
  name: "Name",
  paid: "Paid",
  market: "Market",
  pnl: "P/L",
  pct: "P/L %",
};

function arrow(key) {
  if (state.sortKey !== key) return "";
  return state.sortDir === 1 ? " ▴" : " ▾";
}

function setSort(key) {
  if (state.sortKey === key) {
    state.sortDir = -state.sortDir;
  } else {
    state.sortKey = key;
    state.sortDir = -1;
  }
  notify();
}

// Placeholder thumbnail + 3-letter code, ported from the design's
// thumbOf/code helpers. Exported so the Item Detail screen can reuse the
// same visuals instead of duplicating the logic.
export function thumbOf(item) {
  const h = item.hue;
  return `repeating-linear-gradient(135deg, hsl(${h} 42% 52%) 0 5px, hsl(${(h + 24) % 360} 38% 42%) 5px 10px)`;
}

export function codeOf(item) {
  return item.name
    .replace(/[^A-Za-z ]/g, "")
    .split(" ")
    .map((w) => w[0])
    .filter(Boolean)
    .join("")
    .slice(0, 3)
    .toUpperCase();
}

// Real 90-day history now exists in the DB for seed items, but fetching it
// per row here would be an N+1 invoke() call per table render — a
// deliberate scope cut, not a data gap. The Item Detail screen is what
// consumes the real backfilled history; this row sparkline stays a flat
// two-point line from price_paid to market_price.
function sparkPath(item) {
  const w = 72;
  const h = 20;
  const pad = 3;
  const min = Math.min(item.price_paid, item.market_price);
  const max = Math.max(item.price_paid, item.market_price);
  const span = max - min || 1;
  const y = (v) => h - pad - ((v - min) / span) * (h - pad * 2);
  return `M0 ${y(item.price_paid).toFixed(1)} L${w} ${y(item.market_price).toFixed(1)}`;
}

export function render() {
  ensureItemsLoaded();
  ensureSettingsLoaded();
  ensureGameNamesBackfilled();
  const items = state.items ?? [];

  const games = gamesOf(items);
  const symbol = symbolForCode(state.settings?.steam_currency_code ?? 2);
  const sortLabel = SORT_LABELS[state.sortKey] ?? state.sortKey;

  const winnersBg = state.winners ? "#16241c" : "#15181b";
  const winnersBorder = state.winners ? "#2f5a3f" : "#2b3137";
  const winnersFg = state.winners ? "#69c98a" : "#b9c1c7";

  const totals = deriveTotals(items);
  const pnlPct = totals.invested !== 0 ? (totals.pnl / totals.invested) * 100 : 0;

  const rows = deriveRows(items, {
    query: state.query,
    filter: state.filter,
    winners: state.winners,
    sortKey: state.sortKey,
    sortDir: state.sortDir,
  });

  const root = fromHTML(`
    <div style="flex:none; display:flex; align-items:center; justify-content:space-between; gap:12px; padding:14px 20px 12px; border-bottom:1px solid #1b1f23;">
      <div style="min-width:0;">
        <div style="font-size:20.5px; font-weight:600; letter-spacing:-0.01em;">Portfolio</div>
        <div style="font-size:14px; color:#7d868f; margin-top:2px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;">Live prices from Steam Community Market · updated every 15 min</div>
      </div>
      <div style="display:flex; gap:8px; flex:none;">
        <div id="sl-refresh" class="sl-btn-ghost" style="padding:6px 11px; border:1px solid #2b3137; border-radius:4px; font-size:14.5px; line-height:1.2; white-space:nowrap; color:#b9c1c7; cursor:pointer; background:#15181b; ${state.refreshing ? "pointer-events:none; opacity:0.6;" : ""}">${state.refreshing ? `Refreshing… ${state.refreshProgress ?? ""}` : "Refresh prices"}</div>
        ${
          state.refreshing
            ? `<div id="sl-refresh-cancel" class="sl-btn-ghost" style="padding:6px 11px; border:1px solid #4a2a2a; border-radius:4px; font-size:14.5px; line-height:1.2; white-space:nowrap; color:#e08585; cursor:pointer; background:#1a1213; ${state.refreshCanceling ? "pointer-events:none; opacity:0.6;" : ""}">${state.refreshCanceling ? "Canceling…" : "Cancel"}</div>`
            : ""
        }
        <div id="sl-add" class="sl-btn-accent" style="padding:6px 11px; border:1px solid #d98a52; border-radius:4px; font-size:14.5px; line-height:1.2; white-space:nowrap; color:#0c0e10; background:#d98a52; cursor:pointer; font-weight:600;">+ Add item</div>
      </div>
    </div>

    ${
      state.refreshMessage
        ? `<div style="flex:none; padding:8px 20px; font-size:13.5px; color:#d98a52; border-bottom:1px solid #1b1f23; background:#1a140d;">${state.refreshMessage}</div>`
        : ""
    }

    <div style="flex:none; display:grid; grid-template-columns:repeat(4, minmax(0,1fr)); gap:1px; background:#1b1f23; border-bottom:1px solid #1b1f23;">
      <div style="background:#101315; padding:11px 14px; min-width:0; white-space:nowrap; overflow:hidden;">
        <div style="font-family:'IBM Plex Mono', monospace; font-size:11.5px; letter-spacing:0.12em; color:#5d666d;">INVESTED</div>
        <div style="font-family:'IBM Plex Mono', monospace; font-size:23px; font-weight:600; margin-top:3px;">${money(totals.invested, symbol)}</div>
      </div>
      <div style="background:#101315; padding:11px 14px; min-width:0; white-space:nowrap; overflow:hidden;">
        <div style="font-family:'IBM Plex Mono', monospace; font-size:11.5px; letter-spacing:0.12em; color:#5d666d;">MARKET VALUE</div>
        <div style="font-family:'IBM Plex Mono', monospace; font-size:23px; font-weight:600; margin-top:3px;">${money(totals.value, symbol)}</div>
      </div>
      <div style="background:#101315; padding:11px 14px; min-width:0; white-space:nowrap; overflow:hidden;">
        <div style="font-family:'IBM Plex Mono', monospace; font-size:11.5px; letter-spacing:0.12em; color:#5d666d;">UNREALISED P/L</div>
        <div style="font-family:'IBM Plex Mono', monospace; font-size:20px; font-weight:600; margin-top:3px; color:${col(totals.pnl)};">${smoney(totals.pnl, symbol)}</div>
        <div style="font-family:'IBM Plex Mono', monospace; font-size:13px; color:${col(totals.pnl)};">${(pnlPct >= 0 ? "+" : "−") + Math.abs(pnlPct).toFixed(1)}%</div>
      </div>
      <div style="background:#101315; padding:11px 14px; min-width:0; white-space:nowrap; overflow:hidden;">
        <div style="font-family:'IBM Plex Mono', monospace; font-size:11.5px; letter-spacing:0.12em; color:#5d666d;">REALISED</div>
        <div style="font-family:'IBM Plex Mono', monospace; font-size:23px; font-weight:600; margin-top:3px; color:${col(state.realised)};">${smoney(state.realised, symbol)}</div>
      </div>
    </div>

    <div style="flex:none; display:flex; align-items:center; gap:8px; padding:9px 20px; border-bottom:1px solid #1b1f23; background:#0e1113;">
      <div style="position:relative; flex:1 1 150px; max-width:260px; min-width:110px;">
        <input id="sl-query" value="${state.query}" placeholder="Filter items…   /" style="width:100%; padding:5px 9px; background:#15181b; border:1px solid #2b3137; border-radius:4px; color:#e3e7ea; font-size:14.5px;" />
      </div>
      <select id="sl-filter" style="flex:none; white-space:nowrap; padding:5px 10px; background:#15181b; border:1px solid #2b3137; border-radius:4px; font-size:14.5px; color:#b9c1c7; cursor:pointer;">
        <option value="" ${state.filter === "" ? "selected" : ""}>All apps</option>
        ${games.map((g) => `<option value="${g}" ${state.filter === g ? "selected" : ""}>${g}</option>`).join("")}
      </select>
      <div id="sl-winners" style="flex:none; white-space:nowrap; padding:5px 10px; background:${winnersBg}; border:1px solid ${winnersBorder}; border-radius:4px; font-size:14.5px; color:${winnersFg}; cursor:pointer;">Profitable only</div>
      <div style="flex:1 1 0; min-width:0;"></div>
      <div style="flex:none; white-space:nowrap; font-family:'IBM Plex Mono', monospace; font-size:13px; color:#6f787f;">${rows.length} shown · sorted by ${sortLabel}</div>
    </div>

    <div style="flex:none; display:grid; grid-template-columns:${COLUMNS}; gap:0; padding:0 20px; height:30px; align-items:center; border-bottom:1px solid #23282d; font-family:'IBM Plex Mono', monospace; font-size:11.5px; letter-spacing:0.1em; color:#5d666d;">
      <div></div>
      <div id="sl-sort-name" class="sl-col-sort" style="cursor:pointer;">ITEM${arrow("name")}</div>
      <div>APP</div>
      <div style="text-align:right;">QTY</div>
      <div id="sl-sort-paid" class="sl-col-sort" style="text-align:right; cursor:pointer;">PAID${arrow("paid")}</div>
      <div id="sl-sort-market" class="sl-col-sort" style="text-align:right; cursor:pointer;">MARKET${arrow("market")}</div>
      <div style="text-align:right;">UPDATED</div>
      <div id="sl-sort-pnl" class="sl-col-sort" style="text-align:right; cursor:pointer;">P/L ${symbol}${arrow("pnl")}</div>
      <div id="sl-sort-pct" class="sl-col-sort" style="text-align:right; cursor:pointer;">P/L %${arrow("pct")}</div>
      <div style="text-align:right;">30D</div>
      <div></div>
    </div>

    <div id="sl-table-body" data-scroll-key="portfolio-table" style="flex:1; overflow-y:auto; min-height:0;"></div>

    <div style="flex:none; height:30px; display:flex; align-items:center; gap:16px; padding:0 20px; border-top:1px solid #23282d; background:#101315; font-family:'IBM Plex Mono', monospace; font-size:12.5px; color:#6f787f;">
      <span>↑↓ move</span><span>↵ open</span><span>/ filter</span><span>⌫ remove</span><span style="flex:1;"></span><span>db: ~/.steamledger/vault.db</span>
    </div>
  `);

  root.querySelector("#sl-refresh").addEventListener("click", () => {
    if (state.refreshing) return;
    state.refreshing = true;
    state.refreshProgress = null;
    state.refreshCanceling = false;
    state.refreshMessage = null;
    notify();

    // Scoped to exactly the currently-filtered/searched rows, not every
    // unsold item — so filtering the view and hitting "Refresh prices"
    // refreshes what's actually shown, per the user's expectation.
    const filteredIds = rows.map((r) => r.id);

    // A full-portfolio refresh sleeps ~1.1s between items to respect
    // Steam's rate limit — for a real-sized portfolio that's several
    // minutes end to end. Without this, the button just sits on
    // "Refreshing…" with no sign of life (see commands::refresh_prices_command).
    listen("price-refresh-progress", (event) => {
      state.refreshProgress = event.payload;
      notify();
    }).then((unlisten) => {
      api
        .refreshPrices(filteredIds)
        .then((summary) => {
          // The backend already computes exactly this — surfacing it is the
          // only way to tell "everything actually updated" apart from
          // "every single lookup silently failed" (e.g. Steam rate-limiting
          // this app), which otherwise look identical: no error, no crash,
          // just nothing changing.
          if (summary.canceled) {
            state.refreshMessage = `Canceled — updated ${summary.updated}, ${summary.skipped} not refreshed.`;
          } else if (summary.skipped > 0) {
            state.refreshMessage = `Updated ${summary.updated} of ${summary.updated + summary.skipped} — the rest couldn't be resolved (Steam may be rate-limiting this app right now; try again later).`;
          } else {
            state.refreshMessage = null;
          }
        })
        .catch((err) => {
          console.error("Failed to refresh prices", err);
          state.refreshMessage = `Refresh failed: ${err}`;
        })
        .finally(() => {
          unlisten();
          state.refreshing = false;
          state.refreshProgress = null;
          state.refreshCanceling = false;
          api
            .listItems()
            .then((items) => {
              state.items = items;
              notify();
            })
            .catch((err) => {
              console.error("Failed to reload items after refresh", err);
              notify();
            });
        });
    });
  });

  root.querySelector("#sl-refresh-cancel")?.addEventListener("click", () => {
    if (state.refreshCanceling) return;
    state.refreshCanceling = true;
    notify();
    // Cooperative cancellation — the backend only checks between items, so
    // there's a brief window (up to one item's fetch + rate-limit sleep)
    // where this is requested but not yet honored.
    api.cancelPriceRefresh().catch((err) => console.error("Failed to cancel price refresh", err));
  });

  root.querySelector("#sl-add").addEventListener("click", () => {
    state.screen = "add";
    notify();
  });

  root.querySelector("#sl-query").addEventListener("input", (e) => {
    state.query = e.target.value;
    notify();
  });

  root.querySelector("#sl-filter").addEventListener("change", (e) => {
    state.filter = e.target.value;
    notify();
  });

  root.querySelector("#sl-winners").addEventListener("click", () => {
    state.winners = !state.winners;
    notify();
  });

  root.querySelector("#sl-sort-name").addEventListener("click", () => setSort("name"));
  root.querySelector("#sl-sort-paid").addEventListener("click", () => setSort("paid"));
  root.querySelector("#sl-sort-market").addEventListener("click", () => setSort("market"));
  root.querySelector("#sl-sort-pnl").addEventListener("click", () => setSort("pnl"));
  root.querySelector("#sl-sort-pct").addEventListener("click", () => setSort("pct"));

  const tableBody = root.querySelector("#sl-table-body");
  for (const item of rows) {
    const d = (item.market_price - item.price_paid) * item.quantity;
    // price_paid is 0 for a still-unpriced Steam-import entry (gifted,
    // traded, or awaiting a store-purchase price fill) — a % change against
    // a zero cost basis is undefined, not "-NaN%".
    const pct = item.price_paid !== 0 ? ((item.market_price - item.price_paid) / item.price_paid) * 100 : null;
    const color = col(d);
    const pctLabel = pct == null ? "—" : `${pct >= 0 ? "+" : "−"}${Math.abs(pct).toFixed(1)}%`;

    const rowEl = fromHTML(`
      <div class="sl-table-row" style="display:grid; grid-template-columns:${COLUMNS}; align-items:center; padding:0 20px; height:40px; border-bottom:1px solid #16191c; cursor:pointer; background:#0c0e10;">
        <div style="width:26px; height:26px; border-radius:3px; border:1px solid #2b3137; background:${thumbOf(item)}; display:flex; align-items:center; justify-content:center; font-family:'IBM Plex Mono', monospace; font-size:10px; color:#0c0e10; font-weight:600;">${codeOf(item)}</div>
        <div style="padding-right:12px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;">${item.name}</div>
        <div style="font-family:'IBM Plex Mono', monospace; font-size:12.5px; color:#7d868f; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;">${gameOf(item)}</div>
        <div style="text-align:right; font-family:'IBM Plex Mono', monospace; font-size:14.5px; color:#98a1a8;">×${item.quantity}</div>
        <div style="text-align:right; font-family:'IBM Plex Mono', monospace; font-size:15px; color:#98a1a8;">${money(item.price_paid * item.quantity, symbol)}</div>
        <div style="text-align:right; font-family:'IBM Plex Mono', monospace; font-size:15px;">${money(item.market_price * item.quantity, symbol)}</div>
        <div style="text-align:right; font-family:'IBM Plex Mono', monospace; font-size:12.5px; color:#7d868f;">${timeAgo(item.market_price_updated_at)}</div>
        <div style="text-align:right; font-family:'IBM Plex Mono', monospace; font-size:15px; font-weight:500; color:${color};">${smoney(d, symbol)}</div>
        <div style="text-align:right; font-family:'IBM Plex Mono', monospace; font-size:15px; color:${color};">${pctLabel}</div>
        <div style="display:flex; justify-content:flex-end;">
          <svg width="58" height="20" viewBox="0 0 72 20" fill="none"><path d="${sparkPath(item)}" stroke="${color}" stroke-width="1.25" fill="none" /></svg>
        </div>
        <div class="sl-row-refresh" data-id="${item.id}" title="Refresh this item's price" style="display:flex; align-items:center; justify-content:center; width:22px; height:22px; border-radius:3px; cursor:pointer; color:#7d868f; font-size:14px; ${state.rowRefreshing.includes(item.id) ? "opacity:0.35; pointer-events:none;" : ""}">⟳</div>
      </div>
    `);

    rowEl.querySelector(".sl-table-row").addEventListener("click", () => {
      state.screen = "detail";
      state.selId = item.id;
      notify();
    });

    rowEl.querySelector(".sl-row-refresh").addEventListener("click", (e) => {
      e.stopPropagation();
      const id = item.id;
      if (state.rowRefreshing.includes(id)) return;
      state.rowRefreshing = [...state.rowRefreshing, id];
      notify();
      state.refreshMessage = null;
      api
        .refreshItemPrice(id)
        .then((price) => {
          if (price != null) {
            state.items = state.items.map((it) =>
              it.id === id ? { ...it, market_price: price, market_price_updated_at: new Date().toISOString() } : it,
            );
          } else {
            // Without this, a failed lookup (e.g. Steam rate-limiting this
            // app, or a name Steam doesn't recognise) looks identical to
            // clicking the button and nothing happening at all.
            state.refreshMessage = `Couldn't refresh "${item.name}" — Steam may be rate-limiting this app right now, or the item name isn't resolvable. Try again later.`;
          }
        })
        .catch((err) => {
          console.error("Failed to refresh item price", err);
          state.refreshMessage = `Couldn't refresh "${item.name}": ${err}`;
        })
        .finally(() => {
          state.rowRefreshing = state.rowRefreshing.filter((x) => x !== id);
          notify();
        });
    });

    tableBody.appendChild(rowEl);
  }

  return root;
}
