// Thin wrappers around every Tauri command registered by the backend
// (see src-tauri/src/commands.rs and the `generate_handler!` list in
// src-tauri/src/lib.rs). Screens import from this single module instead of
// calling `invoke()` directly, so the IPC surface stays in one place.
//
// Tauri's default IPC layer camelCases Rust snake_case argument names, so
// e.g. `item_id: i64` on the Rust side is called as `{ itemId }` here.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { logBackendCall } from "./applog.js";

// Every exported function below calls this instead of the raw Tauri
// `invoke` directly — a single interception point that logs every backend
// call (command, args, outcome, duration) to the activity log (see
// applog.js and screens/log.js), without needing a log call at each of the
// ~25 call sites in this file individually.
function invoke(command, args) {
  const startedAt = performance.now();
  return tauriInvoke(command, args).then(
    (result) => {
      logBackendCall(command, args, { ok: true, durationMs: Math.round(performance.now() - startedAt), result });
      return result;
    },
    (error) => {
      logBackendCall(command, args, {
        ok: false,
        durationMs: Math.round(performance.now() - startedAt),
        error: String(error),
      });
      throw error;
    },
  );
}

export const listItems = () => invoke("list_items");

export const getItem = (id) => invoke("get_item", { id });

export const sellItem = (id, soldPrice) =>
  invoke("sell_item", { id, soldPrice });

export const removeItem = (id) => invoke("remove_item", { id });

export const getPriceHistory = (itemId) =>
  invoke("get_price_history", { itemId });

export const getSettings = () => invoke("get_settings");

export const updateSettings = (payload) =>
  invoke("update_settings", { payload });

// Looks up AND persists one item's current market price (market_price,
// market_price_updated_at, and a price_history row).
export const refreshItemPrice = (itemId) =>
  invoke("refresh_item_price_command", { itemId });

// itemIds scopes the refresh to exactly those items — callers should pass
// the currently-filtered/searched row ids, not assume "all".
export const refreshPrices = (itemIds) =>
  invoke("refresh_prices_command", { itemIds });

export const cancelPriceRefresh = () => invoke("cancel_price_refresh");

export const exportItemsCsv = () => invoke("export_items_csv");

export const exportItemsJson = () => invoke("export_items_json");

export const wipeVault = () => invoke("wipe_vault");

export const getVaultFileSize = () => invoke("get_vault_file_size");

export const saveSteamCookie = (cookie) =>
  invoke("save_steam_cookie_command", { cookie });

export const hasSteamCookie = () => invoke("has_steam_cookie_command");

export const clearSteamCookie = () => invoke("clear_steam_cookie_command");

export const saveSteamStoreCookie = (cookie) =>
  invoke("save_steam_store_cookie_command", { cookie });

export const hasSteamStoreCookie = () => invoke("has_steam_store_cookie_command");

export const clearSteamStoreCookie = () => invoke("clear_steam_store_cookie_command");

// help.steampowered.com is a third, independently-scoped Steam login
// domain — used to resolve per-item prices for multi-item ("pack") store
// purchases via the authenticated help-wizard page. Optional: everything
// else in the Steam import flow works without it, packs just stay flagged.
export const saveSteamHelpCookie = (cookie) =>
  invoke("save_steam_help_cookie_command", { cookie });

export const hasSteamHelpCookie = () => invoke("has_steam_help_cookie_command");

export const clearSteamHelpCookie = () => invoke("clear_steam_help_cookie_command");

// Opens an embedded Steam login window and saves whichever of the three
// domain cookies it manages to confirm — resolves to
// { market, store, help, canceled } (all booleans) once the window closes.
// Emits "steam-connect-progress" events throughout (see settings.js).
export const connectSteamAccount = () => invoke("connect_steam_account");

// Resolves to { transactions, price_fills, flagged_packs } — see
// commands::SteamImportPreview.
export const previewSteamImport = () => invoke("preview_steam_import");

export const commitSteamImport = (transactions, priceFills) =>
  invoke("commit_steam_import", { transactions, priceFills });

export const backfillGameNames = () => invoke("backfill_game_names");
