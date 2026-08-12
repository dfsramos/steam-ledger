import { describe, expect, it } from "vitest";
import { buildChartPaths, computePurchaseStats, heldForLabel } from "./detail-derive.js";

describe("buildChartPaths", () => {
  const history = [
    { price: 10, recorded_at: "2026-01-01" },
    { price: 20, recorded_at: "2026-02-01" },
    { price: 15, recorded_at: "2026-03-01" },
  ];

  it("builds a line path with an M start and L segments for each remaining point", () => {
    const { line } = buildChartPaths(history, 12);
    expect(line.startsWith("M")).toBe(true);
    expect((line.match(/L/g) || []).length).toBe(2);
  });

  it("closes the area path down to the chart floor", () => {
    const { area } = buildChartPaths(history, 12);
    expect(area).toContain("240");
    expect(area.trim().endsWith("Z")).toBe(true);
  });

  it("reports high/low from the history, and start/mid dates", () => {
    const { high, low, start, mid } = buildChartPaths(history, 12);
    expect(high).toBe("£20.00");
    expect(low).toBe("£10.00");
    expect(start).toBe("2026-01-01");
    expect(mid).toBe("2026-02-01");
  });

  it("expands the range to include pricePaid so the cost line isn't clipped", () => {
    const { costLine } = buildChartPaths(history, 5);
    // pricePaid (5) is below the history's min (10) — costLine's y should
    // still be a finite, in-range value, not NaN/Infinity from a 0 span.
    const y = Number(costLine.split(" ")[1]);
    expect(Number.isFinite(y)).toBe(true);
  });

  it("handles an empty history without throwing", () => {
    const result = buildChartPaths([], 42);
    expect(result.line).toBeTruthy();
    expect(result.area).toBeTruthy();
  });

  it("handles a single-point history without throwing", () => {
    const result = buildChartPaths([{ price: 30, recorded_at: "2026-01-01" }], 30);
    expect(result.line).toBeTruthy();
    expect(result.high).toBe("£30.00");
  });
});

describe("heldForLabel", () => {
  it("computes a day difference against today", () => {
    const today = new Date();
    const tenDaysAgo = new Date(today.getTime() - 10 * 24 * 60 * 60 * 1000);
    const dateStr = tenDaysAgo.toISOString().slice(0, 10);
    expect(heldForLabel(dateStr)).toBe("10 days");
  });

  it("never returns a negative day count for a future date", () => {
    const today = new Date();
    const tomorrow = new Date(today.getTime() + 24 * 60 * 60 * 1000);
    const dateStr = tomorrow.toISOString().slice(0, 10);
    expect(heldForLabel(dateStr)).toBe("0 days");
  });
});

describe("computePurchaseStats", () => {
  it("computes stats for a profitable item", () => {
    const item = { price_paid: 50, market_price: 75, quantity: 2 };
    expect(computePurchaseStats(item)).toEqual({
      paidPerUnit: 50,
      costBasis: 100,
      marketValue: 150,
      unrealisedPnl: 50,
      unrealisedPct: 50,
    });
  });

  it("computes stats for a losing item", () => {
    const item = { price_paid: 100, market_price: 80, quantity: 1 };
    expect(computePurchaseStats(item)).toEqual({
      paidPerUnit: 100,
      costBasis: 100,
      marketValue: 80,
      unrealisedPnl: -20,
      unrealisedPct: -20,
    });
  });
});
