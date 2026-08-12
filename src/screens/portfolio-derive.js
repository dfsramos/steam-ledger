// Pure derivation logic for the Portfolio screen, ported from the design
// handoff's script block. Kept side-effect-free and separate from
// portfolio.js's DOM code so it's independently testable.

// `symbol` defaults to "£" purely so existing call sites/tests that don't
// pass one keep working — real call sites should always pass the user's
// configured currency symbol (see src/currency.js), not rely on the
// default. Forcing everyone's prices into GBP regardless of their actual
// Steam account currency was a real reported bug.
export function money(n, symbol = "£") {
  return symbol + n.toLocaleString("en-GB", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

export function smoney(n, symbol = "£") {
  return (n >= 0 ? "+" : "−") + money(Math.abs(n), symbol);
}

export function col(n) {
  return n >= 0 ? "#69c98a" : "#e07a7a";
}

// Totals are computed over the FULL item set, not the filtered/sorted rows
// — matches the design, where the stat tiles never move with search/filter.
export function deriveTotals(items) {
  const invested = items.reduce((a, i) => a + i.price_paid * i.quantity, 0);
  const value = items.reduce((a, i) => a + i.market_price * i.quantity, 0);
  return { invested, value, pnl: value - invested };
}

// Still used by the Add/Import screen's manual-entry category picker (item
// *type*, e.g. "AK47"/"Door") — a different dimension from `gameOf` below
// (which game/app an item is from), not replaced by it.
export function categoriesOf(items) {
  return [...new Set(items.map((i) => i.category))].sort();
}

// `game_name` is only reliably populated for items added after this field
// existed (Steam imports always set it; manual/paste-import default to
// "Rust" server-side) — older rows in an existing vault can have it as
// `null`, hence the appid fallback so nothing renders blank.
export function gameOf(item) {
  return item.game_name || `appid ${item.appid}`;
}

export function gamesOf(items) {
  return [...new Set(items.map(gameOf))].sort();
}

// `iso` is the UTC ISO-8601 string written to `market_price_updated_at`
// (`null` until an item's price has ever been refreshed).
export function timeAgo(iso, now = Date.now()) {
  if (!iso) return "never";
  const thenMs = new Date(iso).getTime();
  if (Number.isNaN(thenMs)) return "never";

  const seconds = Math.max(0, Math.floor((now - thenMs) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

const SORT_KEY_FNS = {
  name: (i) => i.name.toLowerCase(),
  paid: (i) => i.price_paid * i.quantity,
  market: (i) => i.market_price * i.quantity,
  pnl: (i) => (i.market_price - i.price_paid) * i.quantity,
  // price_paid can be 0 for a synthetic Steam-import entry with no known
  // purchase (gifted/traded, price filled in later) — treat as neutral
  // rather than NaN, which sorts unpredictably.
  pct: (i) => (i.price_paid === 0 ? 0 : (i.market_price - i.price_paid) / i.price_paid),
};

// `filter` is the exact game name to match (as returned by `gameOf`), or
// "" for "All" — a direct value, not an index, since the filter control is
// a <select> now (see portfolio.js).
export function deriveRows(
  items,
  { query = "", filter = "", winners = false, sortKey = "pnl", sortDir = -1 } = {},
) {
  const normalizedQuery = query.trim().toLowerCase();

  const filtered = items.filter(
    (i) =>
      (!normalizedQuery || i.name.toLowerCase().includes(normalizedQuery)) &&
      (!filter || gameOf(i) === filter) &&
      (!winners || i.market_price > i.price_paid),
  );

  const keyFn = SORT_KEY_FNS[sortKey] ?? SORT_KEY_FNS.pnl;
  return filtered.slice().sort((a, b) => {
    const x = keyFn(a);
    const y = keyFn(b);
    return (x < y ? -1 : x > y ? 1 : 0) * sortDir;
  });
}
