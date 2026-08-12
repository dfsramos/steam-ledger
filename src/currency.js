// Steam's ECurrencyCode -> {iso, symbol}. Confirmed against the public enum
// (node-steam data docs — see .mallet/lessons.md for the research trail),
// not guessed. Symbols disambiguate from USD/other "$" currencies where the
// natural glyph would collide (C$, A$, MX$, ...); currencies with no
// universally-recognized glyph fall back to their ISO code as the
// "symbol" — clear and correct, even if not a native-script character.
export const CURRENCIES = [
  { code: 1, iso: "USD", symbol: "$" },
  { code: 2, iso: "GBP", symbol: "£" },
  { code: 3, iso: "EUR", symbol: "€" },
  { code: 4, iso: "CHF", symbol: "CHF" },
  { code: 5, iso: "RUB", symbol: "₽" },
  { code: 6, iso: "PLN", symbol: "zł" },
  { code: 7, iso: "BRL", symbol: "R$" },
  { code: 8, iso: "JPY", symbol: "¥" },
  { code: 9, iso: "NOK", symbol: "kr" },
  { code: 10, iso: "IDR", symbol: "Rp" },
  { code: 11, iso: "MYR", symbol: "RM" },
  { code: 12, iso: "PHP", symbol: "₱" },
  { code: 13, iso: "SGD", symbol: "S$" },
  { code: 14, iso: "THB", symbol: "฿" },
  { code: 15, iso: "VND", symbol: "₫" },
  { code: 16, iso: "KRW", symbol: "₩" },
  { code: 17, iso: "TRY", symbol: "₺" },
  { code: 18, iso: "UAH", symbol: "₴" },
  { code: 19, iso: "MXN", symbol: "MX$" },
  { code: 20, iso: "CAD", symbol: "C$" },
  { code: 21, iso: "AUD", symbol: "A$" },
  { code: 22, iso: "NZD", symbol: "NZ$" },
  { code: 23, iso: "CNY", symbol: "CN¥" },
  { code: 24, iso: "INR", symbol: "₹" },
  { code: 25, iso: "CLP", symbol: "CLP$" },
  { code: 26, iso: "PEN", symbol: "S/" },
  { code: 27, iso: "COP", symbol: "COL$" },
  { code: 28, iso: "ZAR", symbol: "R" },
  { code: 29, iso: "HKD", symbol: "HK$" },
  { code: 30, iso: "TWD", symbol: "NT$" },
  { code: 31, iso: "SAR", symbol: "SAR" },
  { code: 32, iso: "AED", symbol: "AED" },
  { code: 33, iso: "SEK", symbol: "kr" },
  { code: 34, iso: "ARS", symbol: "AR$" },
  { code: 35, iso: "ILS", symbol: "₪" },
  { code: 37, iso: "KZT", symbol: "₸" },
  { code: 38, iso: "KWD", symbol: "KWD" },
  { code: 39, iso: "QAR", symbol: "QAR" },
  { code: 40, iso: "CRC", symbol: "₡" },
  { code: 41, iso: "UYU", symbol: "$U" },
];

const BY_CODE = new Map(CURRENCIES.map((c) => [c.code, c]));

export function symbolForCode(code) {
  return BY_CODE.get(code)?.symbol ?? "?";
}
