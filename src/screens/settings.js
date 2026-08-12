// Settings screen: market refresh interval, backup path, auto-backup
// toggle, CSV/JSON export, and a destructive two-step vault wipe. The
// design mockup's native "Choose…" folder picker and "Restore from backup"
// are intentionally out of scope — see plan.md's Context.

import { listen } from "@tauri-apps/api/event";
import { state, notify } from "../state.js";
import * as api from "../api.js";
import { ensureSettingsLoaded } from "../items-store.js";
import { CURRENCIES } from "../currency.js";

const INTERVALS = [5, 15, 30, 60];

function fromHTML(html) {
  const template = document.createElement("template");
  template.innerHTML = html.trim();
  return template.content;
}

function placeholder(text) {
  const div = document.createElement("div");
  div.style.cssText = "padding:20px; color:#7d868f;";
  div.textContent = text;
  return div;
}

function saveSettings(patch) {
  state.settings = { ...state.settings, ...patch };
  notify();
  api.updateSettings(state.settings).catch((err) => console.error("Failed to save settings", err));
}

function triggerDownload(text, filename, mimeType) {
  const blob = new Blob([text], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function formatBytes(bytes) {
  return `${(bytes / 1024).toFixed(1)} KB`;
}

export function render() {
  ensureSettingsLoaded();

  if (state.hasSteamCookie === undefined) {
    api.hasSteamCookie().then((v) => {
      state.hasSteamCookie = v;
      notify();
    });
  }

  if (state.hasSteamStoreCookie === undefined) {
    api.hasSteamStoreCookie().then((v) => {
      state.hasSteamStoreCookie = v;
      notify();
    });
  }

  if (state.hasSteamHelpCookie === undefined) {
    api.hasSteamHelpCookie().then((v) => {
      state.hasSteamHelpCookie = v;
      notify();
    });
  }

  if (state.settings === undefined) {
    return placeholder("Loading…");
  }

  const settings = state.settings;
  const autoOn = settings.auto_backup;

  const root = fromHTML(`
    <div style="flex:none; padding:16px 20px 12px; border-bottom:1px solid #1b1f23;">
      <div style="font-size:20.5px; font-weight:600; letter-spacing:-0.01em;">Settings</div>
      <div style="font-size:14px; color:#7d868f; margin-top:2px;">All data stays on this device. Nothing is uploaded.</div>
    </div>
    <div data-scroll-key="settings-content" style="flex:1; overflow-y:auto; min-height:0; padding:18px 20px; max-width:680px;">
      <div style="font-family:'IBM Plex Mono', monospace; font-size:12px; letter-spacing:0.12em; color:#5d666d;">MARKET REFRESH</div>
      <div id="sl-intervals" style="display:flex; gap:6px; margin-top:8px;">
        ${INTERVALS.map((iv) => {
          const active = settings.refresh_interval_minutes === iv;
          const border = active ? "#d98a52" : "#2b3137";
          const bg = active ? "#241a10" : "#15181b";
          const fg = active ? "#d98a52" : "#b9c1c7";
          return `<div class="sl-interval" data-minutes="${iv}" style="padding:6px 13px; border:1px solid ${border}; background:${bg}; color:${fg}; border-radius:4px; font-family:'IBM Plex Mono', monospace; font-size:14.5px; cursor:pointer; white-space:nowrap;">${iv} min</div>`;
        }).join("")}
      </div>
      <div style="font-size:14px; color:#6f787f; margin-top:8px;">Fetches only for items you own. Manual refresh only — no background scheduler yet.</div>

      <div style="height:1px; background:#23282d; margin:20px 0;"></div>

      <div style="font-family:'IBM Plex Mono', monospace; font-size:12px; letter-spacing:0.12em; color:#5d666d;">CURRENCY</div>
      <select id="sl-currency" style="margin-top:8px; padding:7px 10px; background:#15181b; border:1px solid #2b3137; border-radius:4px; color:#e3e7ea; font-family:'IBM Plex Mono', monospace; font-size:14.5px;">
        ${CURRENCIES.map(
          (c) =>
            `<option value="${c.code}" ${settings.steam_currency_code === c.code ? "selected" : ""}>${c.iso} (${c.symbol})</option>`,
        ).join("")}
      </select>
      <div style="font-size:14px; color:#6f787f; margin-top:8px;">Used for both displayed prices and live Steam Market price lookups — should match your actual Steam account's currency, not just your locale.</div>

      <div style="height:1px; background:#23282d; margin:20px 0;"></div>

      <div style="font-family:'IBM Plex Mono', monospace; font-size:12px; letter-spacing:0.12em; color:#5d666d;">BACKUP LOCATION</div>
      <div style="display:flex; gap:8px; margin-top:8px;">
        <input id="sl-backup-path" value="${settings.backup_path}" placeholder="/path/to/backup/folder" style="flex:1; padding:8px 10px; background:#15181b; border:1px solid #2b3137; border-radius:4px; color:#c9d0d5; font-family:'IBM Plex Mono', monospace; font-size:14.5px;" />
      </div>
      <div style="display:flex; align-items:center; gap:10px; margin-top:10px;">
        <div id="sl-auto-backup" style="width:34px; height:19px; border-radius:10px; background:${autoOn ? "#2f5a3f" : "#2b3137"}; position:relative; cursor:pointer; flex:none;">
          <div style="position:absolute; top:2px; left:${autoOn ? "17px" : "2px"}; width:15px; height:15px; border-radius:50%; background:#0c0e10;"></div>
        </div>
        <span style="font-size:15px;">Auto-backup vault.db after every change</span>
      </div>

      <div style="height:1px; background:#23282d; margin:20px 0;"></div>

      <div style="font-family:'IBM Plex Mono', monospace; font-size:12px; letter-spacing:0.12em; color:#5d666d;">CONNECT STEAM ACCOUNT</div>
      <div style="font-size:14px; color:#6f787f; margin-top:6px; line-height:1.6;">Opens a Steam login window in the app. One sign-in connects all three cookies below automatically — no devtools, no manual copy-paste.</div>
      <div style="display:flex; align-items:center; gap:10px; margin-top:8px;">
        <div id="sl-steam-connect" style="padding:8px 16px; background:#d98a52; color:#0c0e10; border-radius:4px; font-size:15px; font-weight:600; cursor:pointer; ${state.steamConnecting ? "opacity:0.6; pointer-events:none;" : ""}">${state.steamConnecting ? "Connecting…" : "Connect Steam Account"}</div>
      </div>
      ${
        state.steamConnectLog.length
          ? `<div data-scroll-key="steam-connect-log" style="margin-top:10px; max-height:110px; overflow-y:auto; border:1px solid #2b3137; border-radius:4px; background:#101315; padding:8px 10px; font-family:'IBM Plex Mono', monospace; font-size:12.5px; color:#7d868f; line-height:1.7;">
              ${state.steamConnectLog.map((line) => `<div>${line}</div>`).join("")}
            </div>`
          : ""
      }
      <div style="font-size:13.5px; color:#5d666d; margin-top:8px; line-height:1.5;">If a domain doesn't connect this way (e.g. it's blocked in the embedded window), paste its cookie manually in the matching section below instead.</div>

      <div style="height:1px; background:#23282d; margin:20px 0;"></div>

      <div style="font-family:'IBM Plex Mono', monospace; font-size:12px; letter-spacing:0.12em; color:#5d666d;">STEAM ACCOUNT</div>
      ${
        state.hasSteamCookie === true
          ? `
      <div style="display:flex; align-items:center; gap:10px; margin-top:8px;">
        <span style="display:flex; align-items:center; gap:6px; padding:6px 12px; border:1px solid #2f5a3f; background:#152018; color:#7fbf8f; border-radius:4px; font-size:14.5px;">
          <span style="width:7px; height:7px; border-radius:50%; background:#4caf6a; display:inline-block;"></span>
          Connected
        </span>
        <div id="sl-steam-cookie-clear" style="padding:7px 13px; border:1px solid #4a2b2b; color:#e07a7a; border-radius:4px; font-size:15px; cursor:pointer; white-space:nowrap;">Clear</div>
      </div>
      `
          : `
      <div style="display:flex; gap:8px; margin-top:8px;">
        <input type="password" id="sl-steam-cookie" placeholder="steamLoginSecure cookie value" autocomplete="off" style="flex:1; padding:8px 10px; background:#15181b; border:1px solid #2b3137; border-radius:4px; color:#c9d0d5; font-family:'IBM Plex Mono', monospace; font-size:14.5px;" />
        <div id="sl-steam-cookie-save" style="padding:7px 13px; border:1px solid #2b3137; border-radius:4px; font-size:15px; color:#b9c1c7; cursor:pointer; white-space:nowrap;">Save</div>
      </div>
      `
      }
      <div style="font-size:14px; color:#6f787f; margin-top:8px;">This is the <code>steamLoginSecure</code> cookie from your browser's Steam session on <strong>steamcommunity.com</strong> (devtools → Application → Cookies → <code>steamcommunity.com</code>). Stored via OS-native credential storage (Keychain/Credential Manager/Secret Service) — never written to the app's database.</div>

      <div style="height:1px; background:#23282d; margin:20px 0;"></div>

      <div style="font-family:'IBM Plex Mono', monospace; font-size:12px; letter-spacing:0.12em; color:#5d666d;">STEAM STORE ACCOUNT</div>
      ${
        state.hasSteamStoreCookie === true
          ? `
      <div style="display:flex; align-items:center; gap:10px; margin-top:8px;">
        <span style="display:flex; align-items:center; gap:6px; padding:6px 12px; border:1px solid #2f5a3f; background:#152018; color:#7fbf8f; border-radius:4px; font-size:14.5px;">
          <span style="width:7px; height:7px; border-radius:50%; background:#4caf6a; display:inline-block;"></span>
          Connected
        </span>
        <div id="sl-steam-store-cookie-clear" style="padding:7px 13px; border:1px solid #4a2b2b; color:#e07a7a; border-radius:4px; font-size:15px; cursor:pointer; white-space:nowrap;">Clear</div>
      </div>
      `
          : `
      <div style="display:flex; gap:8px; margin-top:8px;">
        <input type="password" id="sl-steam-store-cookie" placeholder="steamLoginSecure cookie value" autocomplete="off" style="flex:1; padding:8px 10px; background:#15181b; border:1px solid #2b3137; border-radius:4px; color:#c9d0d5; font-family:'IBM Plex Mono', monospace; font-size:14.5px;" />
        <div id="sl-steam-store-cookie-save" style="padding:7px 13px; border:1px solid #2b3137; border-radius:4px; font-size:15px; color:#b9c1c7; cursor:pointer; white-space:nowrap;">Save</div>
      </div>
      `
      }
      <div style="font-size:14px; color:#6f787f; margin-top:8px;">This is a <em>separate</em> <code>steamLoginSecure</code> cookie value, from your browser's session on <strong>store.steampowered.com</strong> (devtools → Application → Cookies → <code>store.steampowered.com</code> — NOT <code>steamcommunity.com</code>, that's a different cookie above). Steam issues distinct session cookies per domain, so the Steam account cookie above will not work here. Stored via OS-native credential storage — never written to the app's database.</div>

      <div style="height:1px; background:#23282d; margin:20px 0;"></div>

      <div style="font-family:'IBM Plex Mono', monospace; font-size:12px; letter-spacing:0.12em; color:#5d666d;">STEAM SUPPORT ACCOUNT (OPTIONAL)</div>
      ${
        state.hasSteamHelpCookie === true
          ? `
      <div style="display:flex; align-items:center; gap:10px; margin-top:8px;">
        <span style="display:flex; align-items:center; gap:6px; padding:6px 12px; border:1px solid #2f5a3f; background:#152018; color:#7fbf8f; border-radius:4px; font-size:14.5px;">
          <span style="width:7px; height:7px; border-radius:50%; background:#4caf6a; display:inline-block;"></span>
          Connected
        </span>
        <div id="sl-steam-help-cookie-clear" style="padding:7px 13px; border:1px solid #4a2b2b; color:#e07a7a; border-radius:4px; font-size:15px; cursor:pointer; white-space:nowrap;">Clear</div>
      </div>
      `
          : `
      <div style="display:flex; gap:8px; margin-top:8px;">
        <input type="password" id="sl-steam-help-cookie" placeholder="steamLoginSecure cookie value" autocomplete="off" style="flex:1; padding:8px 10px; background:#15181b; border:1px solid #2b3137; border-radius:4px; color:#c9d0d5; font-family:'IBM Plex Mono', monospace; font-size:14.5px;" />
        <div id="sl-steam-help-cookie-save" style="padding:7px 13px; border:1px solid #2b3137; border-radius:4px; font-size:15px; color:#b9c1c7; cursor:pointer; white-space:nowrap;">Save</div>
      </div>
      `
      }
      <div style="font-size:14px; color:#6f787f; margin-top:8px;">A <em>third, separate</em> <code>steamLoginSecure</code> cookie, from your browser's session on <strong>help.steampowered.com</strong> (devtools → Application → Cookies → <code>help.steampowered.com</code>). Optional — only used to look up the per-item price breakdown of a multi-item store purchase ("pack"), which Steam doesn't expose anywhere else. Without it, packs are still imported but flagged for you to price manually. Stored via OS-native credential storage — never written to the app's database.</div>

      <div style="height:1px; background:#23282d; margin:20px 0;"></div>

      <div style="font-family:'IBM Plex Mono', monospace; font-size:12px; letter-spacing:0.12em; color:#5d666d;">DATA</div>
      <div style="display:flex; gap:8px; margin-top:8px; flex-wrap:wrap;">
        <div id="sl-export-csv" style="padding:7px 13px; border:1px solid #2b3137; border-radius:4px; font-size:15px; color:#b9c1c7; cursor:pointer; white-space:nowrap;">Export CSV</div>
        <div id="sl-export-json" style="padding:7px 13px; border:1px solid #2b3137; border-radius:4px; font-size:15px; color:#b9c1c7; cursor:pointer; white-space:nowrap;">Export JSON</div>
        <div id="sl-wipe-vault" style="padding:7px 13px; border:1px solid #4a2b2b; color:#e07a7a; border-radius:4px; font-size:15px; cursor:pointer; white-space:nowrap;">${state.wipeConfirming ? "Confirm wipe?" : "Wipe local vault"}</div>
      </div>
      <div id="sl-vault-info" style="margin-top:22px; padding:11px 13px; border:1px solid #23282d; border-radius:5px; background:#101315; font-family:'IBM Plex Mono', monospace; font-size:14px; color:#7d868f; line-height:1.8;">
        <div>vault&nbsp;&nbsp;&nbsp;&nbsp;~/.steamledger/vault.db · …</div>
        <div>engine&nbsp;&nbsp;&nbsp;rusqlite 0.32 (bundled sqlite)</div>
        <div>build&nbsp;&nbsp;&nbsp;&nbsp;steam-ledger 0.1.0 · tauri 2.11</div>
      </div>
    </div>
  `);

  root.querySelectorAll(".sl-interval").forEach((el) => {
    el.addEventListener("click", () => {
      saveSettings({ refresh_interval_minutes: Number(el.dataset.minutes) });
    });
  });

  root.querySelector("#sl-currency").addEventListener("change", (e) => {
    saveSettings({ steam_currency_code: Number(e.target.value) });
  });

  root.querySelector("#sl-backup-path").addEventListener("blur", (e) => {
    saveSettings({ backup_path: e.target.value });
  });

  root.querySelector("#sl-auto-backup").addEventListener("click", () => {
    saveSettings({ auto_backup: !settings.auto_backup });
  });

  const steamConnect = root.querySelector("#sl-steam-connect");
  if (steamConnect) {
    steamConnect.addEventListener("click", () => {
      state.steamConnecting = true;
      state.steamConnectLog = [];
      notify();

      // Mirrors add.js's Steam-import progress pattern: the backend can take
      // a while (the user has to actually type credentials), so a silent
      // "Connecting…" button with no other feedback reads as hung.
      listen("steam-connect-progress", (event) => {
        state.steamConnectLog = [...state.steamConnectLog, event.payload];
        notify();
      }).then((unlisten) => {
        api
          .connectSteamAccount()
          .then((result) => {
            unlisten();
            state.steamConnecting = false;
            // Re-fetch every domain's status rather than trusting `result`
            // directly — a domain the user had already connected manually
            // stays connected even if this run didn't touch it.
            state.hasSteamCookie = undefined;
            state.hasSteamStoreCookie = undefined;
            state.hasSteamHelpCookie = undefined;
            notify();
          })
          .catch((err) => {
            unlisten();
            console.error("Failed to connect Steam account", err);
            state.steamConnectLog = [...state.steamConnectLog, `Error: ${err}`];
            state.steamConnecting = false;
            notify();
          });
      });
    });
  }

  const steamCookieSave = root.querySelector("#sl-steam-cookie-save");
  if (steamCookieSave) {
    steamCookieSave.addEventListener("click", () => {
      // Query the live document, not `root` — by the time this handler
      // fires, appendChild(root) has already moved this DocumentFragment's
      // children into the DOM and left it empty.
      const input = document.getElementById("sl-steam-cookie");
      const value = input ? input.value : "";
      api
        .saveSteamCookie(value)
        .then(() => {
          state.hasSteamCookie = true;
          notify();
        })
        .catch((err) => console.error("Failed to save Steam cookie", err));
    });
  }

  const steamCookieClear = root.querySelector("#sl-steam-cookie-clear");
  if (steamCookieClear) {
    steamCookieClear.addEventListener("click", () => {
      api
        .clearSteamCookie()
        .then(() => {
          state.hasSteamCookie = false;
          notify();
        })
        .catch((err) => console.error("Failed to clear Steam cookie", err));
    });
  }

  const steamStoreCookieSave = root.querySelector("#sl-steam-store-cookie-save");
  if (steamStoreCookieSave) {
    steamStoreCookieSave.addEventListener("click", () => {
      // Query the live document, not `root` — by the time this handler
      // fires, appendChild(root) has already moved this DocumentFragment's
      // children into the DOM and left it empty.
      const input = document.getElementById("sl-steam-store-cookie");
      const value = input ? input.value : "";
      api
        .saveSteamStoreCookie(value)
        .then(() => {
          state.hasSteamStoreCookie = true;
          notify();
        })
        .catch((err) => console.error("Failed to save Steam store cookie", err));
    });
  }

  const steamStoreCookieClear = root.querySelector("#sl-steam-store-cookie-clear");
  if (steamStoreCookieClear) {
    steamStoreCookieClear.addEventListener("click", () => {
      api
        .clearSteamStoreCookie()
        .then(() => {
          state.hasSteamStoreCookie = false;
          notify();
        })
        .catch((err) => console.error("Failed to clear Steam store cookie", err));
    });
  }

  const steamHelpCookieSave = root.querySelector("#sl-steam-help-cookie-save");
  if (steamHelpCookieSave) {
    steamHelpCookieSave.addEventListener("click", () => {
      const input = document.getElementById("sl-steam-help-cookie");
      const value = input ? input.value : "";
      api
        .saveSteamHelpCookie(value)
        .then(() => {
          state.hasSteamHelpCookie = true;
          notify();
        })
        .catch((err) => console.error("Failed to save Steam help cookie", err));
    });
  }

  const steamHelpCookieClear = root.querySelector("#sl-steam-help-cookie-clear");
  if (steamHelpCookieClear) {
    steamHelpCookieClear.addEventListener("click", () => {
      api
        .clearSteamHelpCookie()
        .then(() => {
          state.hasSteamHelpCookie = false;
          notify();
        })
        .catch((err) => console.error("Failed to clear Steam help cookie", err));
    });
  }

  root.querySelector("#sl-export-csv").addEventListener("click", () => {
    api
      .exportItemsCsv()
      .then((csv) => triggerDownload(csv, "steam-ledger-export.csv", "text/csv"))
      .catch((err) => console.error("Failed to export CSV", err));
  });

  root.querySelector("#sl-export-json").addEventListener("click", () => {
    api
      .exportItemsJson()
      .then((json) => triggerDownload(json, "steam-ledger-export.json", "application/json"))
      .catch((err) => console.error("Failed to export JSON", err));
  });

  root.querySelector("#sl-wipe-vault").addEventListener("click", () => {
    if (!state.wipeConfirming) {
      state.wipeConfirming = true;
      notify();
      setTimeout(() => {
        state.wipeConfirming = false;
        notify();
      }, 3000);
      return;
    }

    state.wipeConfirming = false;
    api
      .wipeVault()
      .then(() => {
        state.items = undefined;
        state.settings = undefined;
        state.screen = "portfolio";
        notify();
      })
      .catch((err) => console.error("Failed to wipe vault", err));
  });

  api
    .getVaultFileSize()
    .then((bytes) => {
      // Query the live document, not `root` — by the time this resolves,
      // appendChild(root) has already moved this DocumentFragment's
      // children into the DOM and left it empty.
      const info = document.getElementById("sl-vault-info");
      if (info) {
        info.innerHTML = `
          <div>vault&nbsp;&nbsp;&nbsp;&nbsp;~/.steamledger/vault.db · ${formatBytes(bytes)}</div>
          <div>engine&nbsp;&nbsp;&nbsp;rusqlite 0.32 (bundled sqlite)</div>
          <div>build&nbsp;&nbsp;&nbsp;&nbsp;steam-ledger 0.1.0 · tauri 2.11</div>
        `;
      }
    })
    .catch((err) => console.error("Failed to get vault file size", err));

  return root;
}
