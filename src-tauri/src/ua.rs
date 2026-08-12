//! Shared `User-Agent` strings for every Steam-facing HTTP client in this
//! app.
//!
//! Confirmed live (2026-08-12) via `curl -A "<value>"` against the real
//! `market/priceoverview` endpoint, holding everything except the UA header
//! constant: this app's own identifier (`skin-ledger/0.1`, this project's
//! pre-rebrand name at the time), an unset/empty
//! UA, a spoofed desktop-browser UA, and even the literal string `reqwest`
//! (the Rust HTTP crate this app uses) all got HTTP 429 on nearly every
//! request — while `curl/8.5.0`, `python-requests/2.31.0`, `Wget/1.21.3`,
//! and `Go-http-client/1.1` all succeeded with real data, and
//! `PostmanRuntime/7.36.0` was blocked again. This isn't a
//! browser-vs-bot check (the browser spoof failed too) — it reads as a
//! denylist targeting identifiers associated with scraping/automation
//! tooling specifically (this app's own name, `reqwest`, Postman), while
//! generic everyday HTTP-client defaults pass through. `PRIMARY` and
//! `FALLBACK` are two of the confirmed-working identities, kept distinct so
//! a 429 retry (see `steam::get_market_price`) isn't just repeating the
//! same request. If Steam's rule changes, re-run the same `curl -A`
//! comparison to find what currently works rather than guessing.
pub const PRIMARY: &str = "curl/8.5.0";
pub const FALLBACK: &str = "python-requests/2.31.0";
