// App shell: title bar + left nav sidebar + vault-value box. Rendered fresh
// on every `notify()` (see src/main.js), so nav highlighting always reflects
// the current `state.screen` without any separate diffing logic.

import { state, notify } from "../state.js";
import { ensureItemsLoaded, ensureSettingsLoaded } from "../items-store.js";
import { col, deriveTotals, money, smoney } from "./portfolio-derive.js";
import { symbolForCode } from "../currency.js";

function fromHTML(html) {
  const template = document.createElement("template");
  template.innerHTML = html.trim();
  return template.content.firstElementChild;
}

const NAV_ITEMS = [
  { key: "portfolio", label: "Portfolio", hint: "1" },
  { key: "add", label: "Add / Import", hint: "2" },
  { key: "detail", label: "Item detail", hint: "3" },
  { key: "settings", label: "Settings", hint: "4" },
  { key: "log", label: "Log", hint: "5" },
];

function navItemHTML(item) {
  const active = state.screen === item.key;
  const bg = active ? "#1b1f23" : "transparent";
  const fg = active ? "#e3e7ea" : "#8b949c";
  const mark = active ? "#d98a52" : "transparent";
  return `
    <div class="sl-nav-item" data-nav="${item.key}" style="display:flex; align-items:center; gap:9px; padding:7px 9px; border-radius:4px; cursor:pointer; background:${bg}; color:${fg};">
      <div style="width:5px; height:14px; border-radius:2px; background:${mark};"></div>
      <span style="flex:1;">${item.label}</span>
      <span style="font-family:'IBM Plex Mono', monospace; font-size:12px; color:#5d666d;">${item.hint}</span>
    </div>
  `;
}

export function render() {
  ensureItemsLoaded();
  ensureSettingsLoaded();
  const items = state.items;
  const loaded = items !== undefined;
  const symbol = symbolForCode(state.settings?.steam_currency_code ?? 2);
  const totals = loaded ? deriveTotals(items) : { invested: 0, value: 0, pnl: 0 };
  const pnlPct = loaded && totals.invested !== 0 ? (totals.pnl / totals.invested) * 100 : 0;

  const vaultValue = loaded ? money(totals.value, symbol) : "—";
  const vaultPnl = loaded ? `${smoney(totals.pnl, symbol)} · ${(pnlPct >= 0 ? "+" : "−") + Math.abs(pnlPct).toFixed(1)}%` : "";
  const vaultPnlColor = loaded ? col(totals.pnl) : "#5d666d";
  const itemCount = loaded ? items.length : "…";

  const root = fromHTML(`
    <div style="height:100vh; min-height:640px; display:flex; flex-direction:column; background:#0c0e10; color:#e3e7ea; font-family:'IBM Plex Sans', system-ui, sans-serif; font-size:15.5px; overflow:hidden;">

      <div style="height:38px; flex:none; display:flex; align-items:center; gap:12px; padding:0 12px; background:#15181b; border-bottom:1px solid #23282d; user-select:none;">
        <div style="display:flex; gap:7px;">
          <div style="width:11px; height:11px; border-radius:50%; background:#3b4147;"></div>
          <div style="width:11px; height:11px; border-radius:50%; background:#3b4147;"></div>
          <div style="width:11px; height:11px; border-radius:50%; background:#3b4147;"></div>
        </div>
        <div style="flex:1; text-align:center; font-size:14px; letter-spacing:0.04em; color:#8b949c;">steam-ledger — local vault</div>
        <div style="display:flex; align-items:center; gap:6px; font-family:'IBM Plex Mono', monospace; font-size:13px; color:#6f787f;">
          <div style="width:6px; height:6px; border-radius:50%; background:#69c98a;"></div>
          <span>synced 2m ago</span>
        </div>
      </div>

      <div style="flex:1; display:flex; min-height:0;">

        <div style="width:200px; flex:none; background:#101315; border-right:1px solid #23282d; display:flex; flex-direction:column; padding:10px 0;">
          <div style="padding:4px 14px 10px; font-family:'IBM Plex Mono', monospace; font-size:12px; letter-spacing:0.14em; color:#5d666d;">NAVIGATE</div>
          <div id="sl-nav-list" style="display:flex; flex-direction:column; gap:1px; padding:0 6px;">
            ${NAV_ITEMS.map(navItemHTML).join("")}
          </div>

          <div style="flex:1;"></div>

          <div style="margin:0 10px; padding:10px; border:1px solid #23282d; border-radius:5px; background:#14171a;">
            <div style="font-family:'IBM Plex Mono', monospace; font-size:11.5px; letter-spacing:0.12em; color:#5d666d;">VAULT VALUE</div>
            <div style="font-family:'IBM Plex Mono', monospace; font-size:24px; font-weight:600; margin-top:4px; letter-spacing:-0.01em;">${vaultValue}</div>
            <div style="font-family:'IBM Plex Mono', monospace; font-size:14px; margin-top:2px; color:${vaultPnlColor};">${vaultPnl}</div>
          </div>
          <div style="padding:10px 14px 2px; font-family:'IBM Plex Mono', monospace; font-size:12px; color:#4e565c;">v0.1.0 · ${itemCount} items</div>
        </div>

        <div id="screen-root" style="flex:1; min-width:0; display:flex; flex-direction:column; background:#0c0e10;"></div>

      </div>
    </div>
  `);

  root.querySelectorAll("[data-nav]").forEach((el) => {
    el.addEventListener("click", () => {
      state.screen = el.dataset.nav;
      notify();
    });
  });

  return root;
}
