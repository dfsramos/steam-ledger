// Item Detail screen: price-history chart, purchase/actions panels, and
// sell/remove/refresh — all wired to live invoke() commands.

import { state, notify } from "../state.js";
import * as api from "../api.js";
import { ensureItemAndHistoryLoaded, ensureSettingsLoaded } from "../items-store.js";
import { col, money, smoney } from "./portfolio-derive.js";
import { codeOf, thumbOf } from "./portfolio.js";
import { buildChartPaths, computePurchaseStats, heldForLabel } from "./detail-derive.js";
import { symbolForCode } from "../currency.js";

function fromHTML(html) {
  const template = document.createElement("template");
  template.innerHTML = html.trim();
  return template.content.firstElementChild;
}

let refreshMessage = null;

function placeholder(text) {
  const div = document.createElement("div");
  div.style.cssText = "padding:20px; color:#7d868f;";
  div.textContent = text;
  return div;
}

function goPortfolio() {
  state.selId = null;
  state.selItem = undefined;
  state.selHistory = undefined;
  state.screen = "portfolio";
  notify();
}

export function render() {
  if (state.selId == null) {
    return placeholder("Select an item from the Portfolio screen to see its details.");
  }

  ensureItemAndHistoryLoaded(state.selId);
  ensureSettingsLoaded();

  if (state.selItem === undefined || state.selItem.id !== state.selId) {
    return placeholder("Loading…");
  }

  const item = state.selItem;
  const history = state.selHistory ?? [];
  const stats = computePurchaseStats(item);
  const symbol = symbolForCode(state.settings?.steam_currency_code ?? 2);
  const chart = buildChartPaths(history, item.price_paid, symbol);

  const root = fromHTML(`
    <div style="display:flex; flex-direction:column; height:100%; min-height:0;">
      <div style="flex:none; display:flex; align-items:center; gap:12px; padding:14px 20px 12px; border-bottom:1px solid #1b1f23;">
        <div id="sl-back" style="padding:5px 9px; border:1px solid #2b3137; border-radius:4px; font-size:14.5px; color:#b9c1c7; cursor:pointer; white-space:nowrap;">← Back</div>
        <div style="width:30px; height:30px; border-radius:4px; border:1px solid #2b3137; background:${thumbOf(item)}; display:flex; align-items:center; justify-content:center; font-family:'IBM Plex Mono', monospace; font-size:11px; color:#0c0e10; font-weight:600;">${codeOf(item)}</div>
        <div style="flex:1;">
          <div style="font-size:19px; font-weight:600;">${item.name}</div>
          <div style="font-family:'IBM Plex Mono', monospace; font-size:13px; color:#7d868f; margin-top:1px;">${item.category} · qty ${item.quantity} · acquired ${item.date_purchased}</div>
        </div>
        <div style="text-align:right;">
          <div style="font-family:'IBM Plex Mono', monospace; font-size:24px; font-weight:600;">${money(item.market_price, symbol)}</div>
          <div style="font-family:'IBM Plex Mono', monospace; font-size:14.5px; color:${col(stats.unrealisedPnl)};">${smoney(stats.unrealisedPnl, symbol)} · ${stats.unrealisedPct >= 0 ? "+" : "−"}${Math.abs(stats.unrealisedPct).toFixed(1)}%</div>
        </div>
      </div>

      <div data-scroll-key="detail-content" style="flex:1; overflow-y:auto; min-height:0; padding:16px 20px;">
        <div style="border:1px solid #23282d; border-radius:5px; background:#101315; padding:14px 16px 8px;">
          <div style="display:flex; align-items:center; justify-content:space-between; margin-bottom:10px;">
            <div style="font-family:'IBM Plex Mono', monospace; font-size:12px; letter-spacing:0.12em; color:#5d666d;">PRICE HISTORY · 90D</div>
            <div style="display:flex; gap:14px; font-family:'IBM Plex Mono', monospace; font-size:12.5px; color:#7d868f;">
              <span>high ${chart.high}</span><span>low ${chart.low}</span><span style="color:#d98a52;">— your cost ${money(item.price_paid, symbol)}</span>
            </div>
          </div>
          <svg viewBox="0 0 900 240" preserveAspectRatio="none" style="width:100%; height:240px; display:block;">
            <path d="${chart.area}" fill="rgba(105,201,138,0.10)" />
            <path d="${chart.line}" stroke="#69c98a" stroke-width="1.6" fill="none" />
            <path d="${chart.costLine}" stroke="#d98a52" stroke-width="1" stroke-dasharray="4 4" fill="none" />
          </svg>
          <div style="display:flex; justify-content:space-between; font-family:'IBM Plex Mono', monospace; font-size:12px; color:#5d666d; padding:4px 0 8px;">
            <span>${chart.start}</span><span>${chart.mid}</span><span>today</span>
          </div>
        </div>

        <div style="display:grid; grid-template-columns:1fr 1fr; gap:14px; margin-top:14px;">
          <div style="border:1px solid #23282d; border-radius:5px; background:#101315; padding:14px 16px;">
            <div style="font-family:'IBM Plex Mono', monospace; font-size:12px; letter-spacing:0.12em; color:#5d666d; margin-bottom:10px;">PURCHASE</div>
            <div style="display:flex; flex-direction:column; gap:7px; font-family:'IBM Plex Mono', monospace; font-size:15px;">
              <div style="display:flex; justify-content:space-between;"><span style="color:#7d868f;">paid / unit</span><span>${money(stats.paidPerUnit, symbol)}</span></div>
              <div style="display:flex; justify-content:space-between;"><span style="color:#7d868f;">quantity</span><span>${item.quantity}</span></div>
              <div style="display:flex; justify-content:space-between;"><span style="color:#7d868f;">cost basis</span><span>${money(stats.costBasis, symbol)}</span></div>
              <div style="display:flex; justify-content:space-between;"><span style="color:#7d868f;">market value</span><span>${money(stats.marketValue, symbol)}</span></div>
              <div style="display:flex; justify-content:space-between;"><span style="color:#7d868f;">held for</span><span>${heldForLabel(item.date_purchased)}</span></div>
              <div style="display:flex; justify-content:space-between; padding-top:7px; border-top:1px solid #23282d;"><span style="color:#7d868f;">unrealised</span><span style="color:${col(stats.unrealisedPnl)}; font-weight:500;">${smoney(stats.unrealisedPnl, symbol)} (${stats.unrealisedPct >= 0 ? "+" : "−"}${Math.abs(stats.unrealisedPct).toFixed(1)}%)</span></div>
            </div>
          </div>
          <div style="border:1px solid #23282d; border-radius:5px; background:#101315; padding:14px 16px; display:flex; flex-direction:column;">
            <div style="font-family:'IBM Plex Mono', monospace; font-size:12px; letter-spacing:0.12em; color:#5d666d; margin-bottom:10px;">ACTIONS</div>
            <div style="display:flex; align-items:center; gap:8px; margin-bottom:10px;">
              <input id="sl-sell-price" value="${state.sellPriceInput}" placeholder="sell price" style="width:110px; padding:7px 9px; background:#15181b; border:1px solid #2b3137; border-radius:4px; color:#e3e7ea; font-family:'IBM Plex Mono', monospace; font-size:15px;" />
              <div id="sl-mark-sold" style="padding:7px 13px; background:#69c98a; color:#0c0e10; border-radius:4px; font-size:15px; font-weight:600; cursor:pointer;">Mark as sold</div>
            </div>
            <div style="font-size:14px; color:#6f787f; line-height:1.6; margin-bottom:auto;">Selling moves the item out of the vault and books the difference into realised P/L. History is kept.</div>
            ${refreshMessage ? `<div style="font-size:13.5px; color:#7d868f; margin-top:8px;">${refreshMessage}</div>` : ""}
            <div style="display:flex; gap:8px; margin-top:12px;">
              <div id="sl-remove" style="padding:7px 13px; border:1px solid #4a2b2b; color:#e07a7a; border-radius:4px; font-size:15px; cursor:pointer; white-space:nowrap;">${state.removeConfirming ? "Confirm remove?" : "Remove from vault"}</div>
              <div id="sl-refresh-price" style="padding:7px 13px; border:1px solid #2b3137; color:#b9c1c7; border-radius:4px; font-size:15px; cursor:pointer; white-space:nowrap;">Refresh price</div>
            </div>
          </div>
        </div>
      </div>
    </div>
  `);

  root.querySelector("#sl-back").addEventListener("click", goPortfolio);

  root.querySelector("#sl-sell-price").addEventListener("input", (e) => {
    state.sellPriceInput = e.target.value;
    notify();
  });

  root.querySelector("#sl-mark-sold").addEventListener("click", () => {
    const soldPrice = parseFloat(state.sellPriceInput);
    if (!Number.isFinite(soldPrice) || soldPrice < 0) return;

    api
      .sellItem(item.id, soldPrice)
      .then(() => {
        state.realised += (soldPrice - item.price_paid) * item.quantity;
        state.sellPriceInput = "";
        state.items = undefined;
        goPortfolio();
      })
      .catch((err) => console.error("Failed to sell item", err));
  });

  root.querySelector("#sl-remove").addEventListener("click", () => {
    if (!state.removeConfirming) {
      state.removeConfirming = true;
      notify();
      setTimeout(() => {
        state.removeConfirming = false;
        notify();
      }, 3000);
      return;
    }

    state.removeConfirming = false;
    api
      .removeItem(item.id)
      .then(() => {
        state.items = undefined;
        goPortfolio();
      })
      .catch((err) => console.error("Failed to remove item", err));
  });

  root.querySelector("#sl-refresh-price").addEventListener("click", () => {
    // Persists (market_price + market_price_updated_at + a price_history
    // row) — otherwise a manually-refreshed price only ever lived in
    // frontend memory and was lost on the next reload.
    api
      .refreshItemPrice(item.id)
      .then((price) => {
        if (price == null) {
          refreshMessage = "No market data for this item.";
        } else {
          refreshMessage = null;
          state.selItem = { ...state.selItem, market_price: price, market_price_updated_at: new Date().toISOString() };
        }
        notify();
      })
      .catch((err) => console.error("Failed to refresh price", err));
  });

  return root;
}
