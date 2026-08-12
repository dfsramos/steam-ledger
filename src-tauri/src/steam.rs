//! Steam Community Market price lookups.

use serde::{Deserialize, Serialize};

/// appid varies per item — 252490 is Rust, this app's default, but items
/// can carry any appid (see the steam-session-import feature). `currency`
/// is Steam's `ECurrencyCode` (1=USD, 2=GBP, 3=EUR, ...), read from
/// `settings.steam_currency_code` by the caller — not hardcoded here, since
/// forcing everyone's prices into GBP was a real reported bug (see
/// `.mallet/lessons.md`). `currency::parse_amount` already strips whatever
/// symbol Steam returns generically, so no parsing change was needed to
/// support this — only which code gets requested.
const PRICEOVERVIEW_URL: &str = "https://steamcommunity.com/market/priceoverview/";

#[derive(Debug, Deserialize)]
struct PriceOverview {
    success: bool,
    lowest_price: Option<String>,
    median_price: Option<String>,
}

enum PriceError {
    RateLimited,
    Other,
}

async fn fetch_price_once(item_name: &str, appid: &str, currency: &str, user_agent: &str) -> Result<f64, PriceError> {
    let url = reqwest::Url::parse_with_params(
        PRICEOVERVIEW_URL,
        &[
            ("appid", appid),
            ("currency", currency),
            ("market_hash_name", item_name),
        ],
    )
    .map_err(|_| PriceError::Other)?;

    // Confirmed live: sending `Accept-Encoding: gzip` on this specific
    // endpoint makes Steam's edge return 429 on nearly every request — the
    // crate-wide `gzip` Cargo feature (needed by inventory.rs for a
    // different endpoint that DOES send compressed error bodies) sends this
    // header by default on every client unless explicitly disabled here.
    // Reproduced independently of reqwest entirely: plain `curl` against the
    // same URL succeeds repeatedly, but starts failing with 429 the moment
    // `Accept-Encoding: gzip` is added (`curl --compressed` or an explicit
    // `-H "Accept-Encoding: gzip"`) — this is Steam-side behavior, not a
    // reqwest/TLS-fingerprint quirk (also ruled out `rustls-tls` vs
    // `native-tls` directly; both 429'd identically until this fix).
    let client = reqwest::Client::builder()
        .user_agent(user_agent)
        .gzip(false)
        .build()
        .map_err(|_| PriceError::Other)?;

    let response = client.get(url).send().await.map_err(|_| PriceError::Other)?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(PriceError::RateLimited);
    }
    if !response.status().is_success() {
        return Err(PriceError::Other);
    }

    let body: PriceOverview = response.json().await.map_err(|_| PriceError::Other)?;
    if !body.success {
        return Err(PriceError::Other);
    }

    body.lowest_price
        .as_deref()
        .and_then(crate::currency::parse_amount)
        .or_else(|| body.median_price.as_deref().and_then(crate::currency::parse_amount))
        .ok_or(PriceError::Other)
}

/// Looks up an item's current Steam Community Market price. Returns `None`
/// on any network error, non-2xx status, `success: false`, JSON-parse
/// failure, or missing/unparseable price fields — including the common case
/// of `item_name` not matching a real Steam `market_hash_name`.
///
/// The dominant cause of a 429 here was the `Accept-Encoding: gzip` header
/// (now disabled on this client — see `fetch_price_once`), but genuine
/// volume-based throttling on top of that is still plausible for a
/// large-portfolio bulk refresh, so a 429 is retried once after a short
/// backoff rather than assumed permanent — a second 429 gives up and
/// returns `None` like any other failure. The retry also switches
/// `User-Agent` (see `ua.rs`): confirmed live that this same endpoint
/// returned 429 for `crate::ua::PRIMARY`-style requests while an identical
/// request moments apart with a different UA succeeded — i.e. the block is
/// at least partly keyed on the UA string looking like an obviously
/// automated client, so retrying with the *same* UA that just got
/// rate-limited is less likely to help than trying a different one.
pub async fn get_market_price(item_name: &str, appid: &str, currency: &str) -> Option<f64> {
    match fetch_price_once(item_name, appid, currency, crate::ua::PRIMARY).await {
        Ok(price) => Some(price),
        Err(PriceError::RateLimited) => {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            fetch_price_once(item_name, appid, currency, crate::ua::FALLBACK).await.ok()
        }
        Err(PriceError::Other) => None,
    }
}

// Bulk refresh (sequential fetch + 1.1s rate-limit sleep + persistence +
// cooperative cancellation) lives in `commands::refresh_prices_command` —
// interleaving the DB write with each fetch (so a cancelled run keeps
// whatever progress it made) needs the DB connection in the same loop, which
// doesn't fit this module's plain-data, DB-agnostic shape.
#[derive(Debug, Clone, Serialize)]
pub struct RefreshSummary {
    pub updated: i64,
    pub skipped: i64,
    pub canceled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolves_a_real_rust_market_item() {
        let price = get_market_price("Whiteout Facemask", "252490", "2").await;
        assert!(price.is_some(), "expected a resolved price for a real market item");
    }

    #[tokio::test]
    async fn returns_none_for_a_nonexistent_item() {
        let price = get_market_price("Definitely Not A Real Item Xyzzy123", "252490", "2").await;
        assert_eq!(price, None);
    }
}
