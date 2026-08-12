// Pure derivation logic for the Item Detail screen. Kept side-effect-free
// and separate from detail.js's DOM code so it's independently testable,
// mirroring portfolio-derive.js's pattern.

import { money } from "./portfolio-derive.js";

const CHART_WIDTH = 900;
const CHART_HEIGHT = 240;

// Builds the SVG path/label data for the price-history chart
// (viewBox="0 0 900 240"). `history` is an array of {price, recorded_at}
// sorted ascending by date. `pricePaid` is folded into the y-range so the
// dashed cost-basis line is never clipped. `symbol` defaults to "£" so
// existing call sites/tests keep working — real callers should pass the
// user's configured currency symbol (see src/currency.js).
export function buildChartPaths(history, pricePaid, symbol = "£") {
  if (history.length === 0) {
    const flatY = CHART_HEIGHT / 2;
    return {
      area: `M0 ${flatY} L${CHART_WIDTH} ${flatY} L${CHART_WIDTH} ${CHART_HEIGHT} L0 ${CHART_HEIGHT} Z`,
      line: `M0 ${flatY} L${CHART_WIDTH} ${flatY}`,
      costLine: `M0 ${flatY} L${CHART_WIDTH} ${flatY}`,
      high: money(pricePaid ?? 0, symbol),
      low: money(pricePaid ?? 0, symbol),
      start: "",
      mid: "",
    };
  }

  const prices = history.map((p) => p.price);
  const min = Math.min(...prices, pricePaid);
  const max = Math.max(...prices, pricePaid);
  const span = max - min || 1;

  const x = (index) =>
    history.length === 1 ? 0 : (index / (history.length - 1)) * CHART_WIDTH;
  const y = (price) => CHART_HEIGHT - ((price - min) / span) * CHART_HEIGHT;

  const linePoints = history.map((p, i) => `${i === 0 ? "M" : "L"}${x(i).toFixed(1)} ${y(p.price).toFixed(1)}`);
  const line = linePoints.join(" ");
  const lastX = x(history.length - 1).toFixed(1);
  const area = `${line} L${lastX} ${CHART_HEIGHT} L0 ${CHART_HEIGHT} Z`;
  const costY = y(pricePaid).toFixed(1);
  const costLine = `M0 ${costY} L${CHART_WIDTH} ${costY}`;

  return {
    area,
    line,
    costLine,
    high: money(Math.max(...prices), symbol),
    low: money(Math.min(...prices), symbol),
    start: history[0].recorded_at,
    mid: history[Math.floor(history.length / 2)].recorded_at,
  };
}

// e.g. "142 days" — day difference between datePurchased ("YYYY-MM-DD") and
// today.
export function heldForLabel(datePurchased) {
  const purchased = new Date(`${datePurchased}T00:00:00Z`);
  const today = new Date();
  const todayUtc = new Date(Date.UTC(today.getFullYear(), today.getMonth(), today.getDate()));
  const days = Math.round((todayUtc - purchased) / (1000 * 60 * 60 * 24));
  return `${Math.max(days, 0)} days`;
}

export function computePurchaseStats(item) {
  const paidPerUnit = item.price_paid;
  const costBasis = item.price_paid * item.quantity;
  const marketValue = item.market_price * item.quantity;
  const unrealisedPnl = marketValue - costBasis;
  const unrealisedPct = item.price_paid !== 0 ? ((item.market_price - item.price_paid) / item.price_paid) * 100 : 0;

  return { paidPerUnit, costBasis, marketValue, unrealisedPnl, unrealisedPct };
}
