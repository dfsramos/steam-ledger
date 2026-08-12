// Hand-rolled reactive core. No framework: screens read `state` directly when
// rendering, mutate it in event handlers, then call `notify()` to trigger a
// re-render of every subscriber (see src/main.js).

export const state = {
  screen: "portfolio",
  selId: null,
  sortKey: "pnl",
  sortDir: -1,
  query: "",
  // Empty string = "All apps"; otherwise an exact game name (see
  // portfolio-derive.js's gameOf) selected from the Portfolio filter
  // dropdown.
  filter: "",
  winners: false,
  // Undefined until the portfolio screen's first live invoke() resolves.
  items: undefined,
  // Running total, bumped by the Item Detail screen's "Mark as sold" action.
  realised: 0,
  refreshing: false,
  // "<done>/<total>" from the backend's price-refresh-progress events, or
  // null before the first one arrives — a full-portfolio refresh sleeps
  // ~1.1s between items to respect Steam's rate limit, so this is what
  // shows movement instead of a silent multi-minute wait.
  refreshProgress: null,
  // True once "Cancel" has been clicked, until the in-flight refresh
  // actually returns — cancellation is cooperative (checked between items
  // on the backend), so there's a brief window where it's requested but not
  // yet honored.
  refreshCanceling: false,
  // Item ids currently mid-refresh via the Portfolio row-level action —
  // lets that row's own button show/disable independently of the others.
  rowRefreshing: [],
  // Result/failure message from the last bulk or per-row price refresh —
  // null when there's nothing to show. Without this, a refresh that fails
  // for every item (e.g. Steam rate-limiting this app) is indistinguishable
  // from clicking the button and nothing happening.
  refreshMessage: null,

  // Add/Import screen (Steam import).
  steamImportRows: [],
  steamImportLoading: false,
  steamImportFilter: "rust",
  steamImportLog: [],
  // Store-purchase price fills + flagged pack rows from preview_steam_import
  // — see add.js's Steam import tab. Empty when no store cookie is saved.
  steamPriceFills: [],
  steamFlaggedPacks: [],

  // Item Detail screen.
  selItem: undefined,
  selHistory: undefined,
  sellPriceInput: "",
  removeConfirming: false,

  // Settings screen.
  settings: undefined,
  wipeConfirming: false,
  // Undefined until the settings screen's first api.hasSteamCookie() resolves.
  hasSteamCookie: undefined,
  // Undefined until the settings screen's first api.hasSteamStoreCookie() resolves.
  hasSteamStoreCookie: undefined,
  // Undefined until the settings screen's first api.hasSteamHelpCookie() resolves.
  hasSteamHelpCookie: undefined,
  // True while the embedded Steam login window (api.connectSteamAccount) is
  // open and being polled for cookies.
  steamConnecting: false,
  // Progress lines from "steam-connect-progress" events, same log-panel
  // pattern as the Steam import tab's steamImportLog.
  steamConnectLog: [],

  // Log screen — see applog.js. The actual log entries live in applog.js's
  // module-level array (not here), since they're append-only infrastructure
  // rather than data a screen derives its render from; this is just the
  // "Copied!" button-label transient.
  logCopyLabel: null,
};

export const subscribers = [];

export function subscribe(fn) {
  subscribers.push(fn);
}

export function notify() {
  for (const fn of subscribers) {
    fn(state);
  }
}
