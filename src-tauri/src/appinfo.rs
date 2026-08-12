//! Resolves a Steam appid to its real store name via Steam's public,
//! unauthenticated `appdetails` API — used to replace numeric-appid
//! fallbacks with readable names wherever an item's real game name isn't
//! otherwise known (no matching transaction in history to read
//! `market_listing_game_name` from).

use std::collections::HashMap;

use serde::Deserialize;

const APPDETAILS_URL: &str = "https://store.steampowered.com/api/appdetails";

#[derive(Debug, Deserialize)]
struct AppDetailsEntry {
    success: bool,
    #[serde(default)]
    data: Option<AppDetailsData>,
}

#[derive(Debug, Deserialize)]
struct AppDetailsData {
    name: String,
}

/// Looks up one appid's real store name. Returns `None` on any failure —
/// network error, non-2xx, or `success: false` (confirmed live: an
/// unresolvable appid returns `{"<id>":{"success":false}}` with no `data`
/// field at all, e.g. a delisted or invalid appid).
pub async fn fetch_app_name(appid: i64) -> Option<String> {
    let client = reqwest::Client::builder()
        .user_agent(crate::ua::PRIMARY)
        .build()
        .ok()?;

    let response = client
        .get(APPDETAILS_URL)
        .query(&[("appids", appid.to_string()), ("filters", "basic".to_string())])
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let body: HashMap<String, AppDetailsEntry> = response.json().await.ok()?;
    let entry = body.get(&appid.to_string())?;
    if !entry.success {
        return None;
    }
    entry.data.as_ref().map(|d| d.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolves_a_real_appid_to_its_store_name() {
        let name = fetch_app_name(252490).await;
        assert_eq!(name, Some("Rust".to_string()));
    }

    #[tokio::test]
    async fn returns_none_for_an_unresolvable_appid() {
        let name = fetch_app_name(999_999_999).await;
        assert_eq!(name, None);
    }
}
