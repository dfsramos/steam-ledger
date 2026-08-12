//! Fetch + parse of the Steam Community inventory API
//! (`/inventory/{steamid}/{appid}/{contextid}`) — used as the ground truth
//! for what the user currently holds, since market history alone can't
//! answer "what do I still own" (an item bought then later resold still
//! shows up as a purchase forever). See
//! `.mallet/features/steam-session-import/state.md` for the full design
//! reasoning behind pairing this with history for price/date.

use std::collections::HashMap;

use serde::Deserialize;

/// Confirmed live against a real account: `count=5000` returns HTTP 400
/// (body `null`), `count=2000` succeeds. Using 2000 as the safe page size.
const PAGE_SIZE: i64 = 2000;

#[derive(Debug, Deserialize)]
struct InventoryAsset {
    classid: String,
    instanceid: String,
    /// A JSON string in the real response (`"1"`), not a number.
    amount: String,
}

#[derive(Debug, Deserialize)]
struct InventoryDescription {
    classid: String,
    instanceid: String,
    market_hash_name: String,
    /// `0`/`1` in the real response, not a JSON bool. Confirmed live: a
    /// non-tradeable bundle/pack item ("Storage Box Pack") reads `0` here.
    /// Not the whole story — see `HoldingInfo::any_marketable`.
    #[serde(default)]
    marketable: i64,
}

/// What's known about one currently-held item type after tallying a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoldingInfo {
    pub count: i64,
    /// True if at least one held unit is marketable *right now*.
    ///
    /// This alone can't distinguish "genuinely non-tradeable" (a bundle
    /// wrapper, quest item) from "temporarily locked" (Steam's post-
    /// purchase/trade market hold, commonly up to 7 days) — both read
    /// `marketable: 0`. `reconcile` resolves the ambiguity by never using
    /// this flag to exclude an item that already has a matching purchase in
    /// history: a hold doesn't erase the fact you bought it, so a
    /// just-bought, still-locked item is included via its history match
    /// regardless of what this flag says. This flag only matters for items
    /// with *no* purchase record at all (gifted/traded/crafted) — there,
    /// `false` means "skip it, not portfolio-relevant" rather than
    /// fabricating an unknown-price placeholder for likely junk.
    pub any_marketable: bool,
}

#[derive(Debug, Default, Deserialize)]
struct InventoryResponse {
    #[serde(default)]
    assets: Vec<InventoryAsset>,
    #[serde(default)]
    descriptions: Vec<InventoryDescription>,
    /// Type not confirmed live (no account with >2000 items in one bucket
    /// was available to test against) — Valve's documented shape uses a
    /// truthy value here, but whether that's a JSON bool or number varies
    /// across Steam endpoints, so this accepts either rather than assuming.
    #[serde(default)]
    more_items: Option<serde_json::Value>,
    #[serde(default)]
    last_assetid: Option<String>,
}

fn is_truthy(value: &serde_json::Value) -> bool {
    value.as_bool().unwrap_or(false) || value.as_i64().unwrap_or(0) != 0
}

/// Sums each asset's `amount` into `holdings`, keyed by `market_hash_name`,
/// joining `assets` to `descriptions` via `(classid, instanceid)` — the
/// same join key Steam uses. An asset with no matching description is
/// skipped rather than erroring (defensive: a real response shouldn't have
/// this happen, but silently dropping one unmatched item is safer than
/// failing the whole import over it).
fn tally_holdings(page: &InventoryResponse, holdings: &mut HashMap<String, HoldingInfo>) {
    let mut descriptions_by_key: HashMap<(&str, &str), (&str, bool)> = HashMap::new();
    for d in &page.descriptions {
        descriptions_by_key.insert(
            (d.classid.as_str(), d.instanceid.as_str()),
            (d.market_hash_name.as_str(), d.marketable != 0),
        );
    }

    for a in &page.assets {
        if let Some(&(name, marketable)) = descriptions_by_key.get(&(a.classid.as_str(), a.instanceid.as_str())) {
            let amount: i64 = a.amount.parse().unwrap_or(1);
            let entry = holdings
                .entry(name.to_string())
                .or_insert(HoldingInfo { count: 0, any_marketable: false });
            entry.count += amount;
            entry.any_marketable |= marketable;
        }
    }
}

async fn fetch_page(
    cookie: &str,
    steamid: &str,
    appid: i64,
    contextid: &str,
    start_assetid: Option<&str>,
) -> Result<InventoryResponse, String> {
    let mut url = format!(
        "https://steamcommunity.com/inventory/{steamid}/{appid}/{contextid}?l=english&count={PAGE_SIZE}"
    );
    if let Some(cursor) = start_assetid {
        url.push_str(&format!("&start_assetid={cursor}"));
    }

    let client = reqwest::Client::builder()
        .user_agent(crate::ua::PRIMARY)
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let response = client
        .get(&url)
        .header("Cookie", format!("steamLoginSecure={cookie}"))
        .send()
        .await
        .map_err(|e| format!("inventory request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "inventory request for appid {appid} contextid {contextid} failed with status {}",
            response.status()
        ));
    }

    response
        .json::<InventoryResponse>()
        .await
        .map_err(|e| format!("failed to parse inventory response: {e}"))
}

/// Fetches the full (paginated) inventory for one `(appid, contextid)` and
/// returns per-item holding info keyed by `market_hash_name`. An empty
/// result is a valid outcome (the user holds nothing for this game/context
/// right now), not an error.
pub async fn fetch_holdings(
    cookie: &str,
    steamid: &str,
    appid: i64,
    contextid: &str,
    progress: Option<&crate::progress::ProgressSender>,
) -> Result<HashMap<String, HoldingInfo>, String> {
    let mut holdings = HashMap::new();
    let mut start_assetid: Option<String> = None;
    let mut page_num = 1;

    loop {
        crate::progress::report(
            progress,
            format!("Checking inventory for appid {appid} (page {page_num})..."),
        );
        let page = fetch_page(cookie, steamid, appid, contextid, start_assetid.as_deref()).await?;
        tally_holdings(&page, &mut holdings);

        match (&page.more_items, &page.last_assetid) {
            (Some(more), Some(cursor)) if is_truthy(more) => {
                start_assetid = Some(cursor.clone());
                page_num += 1;
            }
            _ => break,
        }
    }

    Ok(holdings)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/sample-inventory-response.json");

    #[test]
    fn tallies_holdings_from_the_real_fixture() {
        let page: InventoryResponse = serde_json::from_str(FIXTURE).expect("fixture is valid JSON");
        let mut holdings = HashMap::new();
        tally_holdings(&page, &mut holdings);

        // Confirmed live: 278 asset entries, but one has amount="2", so the
        // summed total is 279, not 278 — real evidence that stacked amounts
        // do occur and must be summed, not just counted per asset entry.
        let total: i64 = holdings.values().map(|h| h.count).sum();
        assert_eq!(total, 279);
        // Confirmed live: "Storage Box Pack"'s real description has
        // marketable: 0 — a genuinely non-tradeable bundle-wrapper item.
        assert_eq!(
            holdings.get("Storage Box Pack"),
            Some(&HoldingInfo { count: 1, any_marketable: false })
        );
    }

    #[test]
    fn sums_amount_across_multiple_assets_sharing_a_classid_and_instanceid() {
        let page = InventoryResponse {
            assets: vec![
                InventoryAsset { classid: "1".into(), instanceid: "0".into(), amount: "3".into() },
                InventoryAsset { classid: "1".into(), instanceid: "0".into(), amount: "2".into() },
            ],
            descriptions: vec![InventoryDescription {
                classid: "1".into(),
                instanceid: "0".into(),
                market_hash_name: "Test Item".into(),
                marketable: 1,
            }],
            more_items: None,
            last_assetid: None,
        };
        let mut holdings = HashMap::new();
        tally_holdings(&page, &mut holdings);
        assert_eq!(holdings.get("Test Item"), Some(&HoldingInfo { count: 5, any_marketable: true }));
    }

    #[test]
    fn skips_an_asset_with_no_matching_description() {
        let page = InventoryResponse {
            assets: vec![InventoryAsset {
                classid: "no-match".into(),
                instanceid: "0".into(),
                amount: "1".into(),
            }],
            descriptions: vec![],
            more_items: None,
            last_assetid: None,
        };
        let mut holdings = HashMap::new();
        tally_holdings(&page, &mut holdings);
        assert!(holdings.is_empty());
    }

    #[test]
    fn any_marketable_is_true_if_at_least_one_held_unit_is_currently_marketable() {
        // Models a temporary post-purchase hold: two copies of the same
        // item, one still locked (just bought), one already unlocked.
        let page = InventoryResponse {
            assets: vec![
                InventoryAsset { classid: "1".into(), instanceid: "0".into(), amount: "1".into() },
                InventoryAsset { classid: "2".into(), instanceid: "0".into(), amount: "1".into() },
            ],
            descriptions: vec![
                InventoryDescription {
                    classid: "1".into(),
                    instanceid: "0".into(),
                    market_hash_name: "Widget".into(),
                    marketable: 0,
                },
                InventoryDescription {
                    classid: "2".into(),
                    instanceid: "0".into(),
                    market_hash_name: "Widget".into(),
                    marketable: 1,
                },
            ],
            more_items: None,
            last_assetid: None,
        };
        let mut holdings = HashMap::new();
        tally_holdings(&page, &mut holdings);
        assert_eq!(holdings.get("Widget"), Some(&HoldingInfo { count: 2, any_marketable: true }));
    }

    #[test]
    fn is_truthy_accepts_both_bool_and_numeric_json_representations() {
        assert!(is_truthy(&serde_json::json!(true)));
        assert!(is_truthy(&serde_json::json!(1)));
        assert!(!is_truthy(&serde_json::json!(false)));
        assert!(!is_truthy(&serde_json::json!(0)));
    }
}
