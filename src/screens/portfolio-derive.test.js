import { describe, expect, it } from "vitest";
import { categoriesOf, col, deriveRows, deriveTotals, gamesOf, money, smoney, timeAgo } from "./portfolio-derive.js";

const ITEMS = [
  { id: 1, name: "Tempered AK47", category: "AK47", quantity: 1, price_paid: 58.4, market_price: 132.9, appid: 252490, game_name: "Rust" },
  { id: 2, name: "Alukiller AK47", category: "AK47", quantity: 2, price_paid: 27.5, market_price: 21.4, appid: 252490, game_name: "Rust" },
  { id: 3, name: "Whiteout Hoodie", category: "Hoodie", quantity: 3, price_paid: 9.2, market_price: 14.8, appid: 440, game_name: "Team Fortress 2" },
];

describe("deriveTotals", () => {
  it("sums invested/value/pnl over the full item set", () => {
    const totals = deriveTotals(ITEMS);
    expect(totals.invested).toBeCloseTo(58.4 * 1 + 27.5 * 2 + 9.2 * 3);
    expect(totals.value).toBeCloseTo(132.9 * 1 + 21.4 * 2 + 14.8 * 3);
    expect(totals.pnl).toBeCloseTo(totals.value - totals.invested);
  });
});

describe("categoriesOf", () => {
  it("returns the sorted distinct category list", () => {
    expect(categoriesOf(ITEMS)).toEqual(["AK47", "Hoodie"]);
  });
});

describe("gamesOf", () => {
  it("returns the sorted distinct game name list", () => {
    expect(gamesOf(ITEMS)).toEqual(["Rust", "Team Fortress 2"]);
  });

  it("falls back to the appid when game_name is null", () => {
    expect(gamesOf([{ appid: 753, game_name: null }])).toEqual(["appid 753"]);
  });
});

describe("deriveRows", () => {
  it("sorts by name ascending", () => {
    const rows = deriveRows(ITEMS, { sortKey: "name", sortDir: 1 });
    expect(rows.map((r) => r.name)).toEqual(["Alukiller AK47", "Tempered AK47", "Whiteout Hoodie"]);
  });

  it("sorts by pnl descending by default", () => {
    const rows = deriveRows(ITEMS, { sortKey: "pnl", sortDir: -1 });
    // pnl: Tempered +74.5, Alukiller -12.2, Whiteout +16.8
    expect(rows.map((r) => r.id)).toEqual([1, 3, 2]);
  });

  it("filters by case-insensitive query", () => {
    const rows = deriveRows(ITEMS, { query: "hood" });
    expect(rows.map((r) => r.id)).toEqual([3]);
  });

  it("filters by exact game name, empty string means All", () => {
    const rows = deriveRows(ITEMS, { filter: "Rust" });
    expect(rows.every((r) => r.game_name === "Rust")).toBe(true);
    expect(rows).toHaveLength(2);

    expect(deriveRows(ITEMS, { filter: "" })).toHaveLength(3);
  });

  it("filters to profitable-only when winners is true", () => {
    const rows = deriveRows(ITEMS, { winners: true });
    expect(rows.map((r) => r.id).sort()).toEqual([1, 3]);
  });

  it("sorts by pct with a zero price_paid item treated as neutral (0%), not NaN", () => {
    const withUnpriced = [
      ...ITEMS,
      { id: 4, name: "Gifted Skin", category: "Misc", quantity: 1, price_paid: 0, market_price: 0, appid: 252490, game_name: "Rust" },
    ];
    const rows = deriveRows(withUnpriced, { sortKey: "pct", sortDir: -1 });
    // pct: Tempered +127.6%, Whiteout +60.9%, Gifted 0% (guarded), Alukiller -22.2%
    expect(rows.map((r) => r.id)).toEqual([1, 3, 4, 2]);
  });
});

describe("formatting helpers", () => {
  it("money formats to 2 decimal places with a pound sign by default", () => {
    expect(money(1234.5)).toBe("£1,234.50");
  });

  it("money uses whatever symbol is passed", () => {
    expect(money(1234.5, "$")).toBe("$1,234.50");
    expect(money(1234.5, "CN¥")).toBe("CN¥1,234.50");
  });

  it("smoney signs positive and negative values", () => {
    expect(smoney(10)).toBe("+£10.00");
    expect(smoney(-10)).toBe("−£10.00");
    expect(smoney(10, "$")).toBe("+$10.00");
  });

  it("col returns green for >= 0 and red for negative", () => {
    expect(col(0)).toBe("#69c98a");
    expect(col(5)).toBe("#69c98a");
    expect(col(-5)).toBe("#e07a7a");
  });
});

describe("timeAgo", () => {
  const now = new Date("2026-08-11T12:00:00Z").getTime();

  it("returns 'never' when never refreshed", () => {
    expect(timeAgo(null, now)).toBe("never");
    expect(timeAgo(undefined, now)).toBe("never");
  });

  it("returns 'just now' for under a minute", () => {
    expect(timeAgo("2026-08-11T11:59:45Z", now)).toBe("just now");
  });

  it("formats minutes/hours/days ago", () => {
    expect(timeAgo("2026-08-11T11:55:00Z", now)).toBe("5m ago");
    expect(timeAgo("2026-08-11T09:00:00Z", now)).toBe("3h ago");
    expect(timeAgo("2026-08-09T12:00:00Z", now)).toBe("2d ago");
  });

  it("returns 'never' for an unparseable timestamp instead of NaN-based output", () => {
    expect(timeAgo("not-a-date", now)).toBe("never");
  });
});
