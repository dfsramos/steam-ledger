//! Authenticated fetch + parse of the Steam Community Market history
//! endpoint (`myhistory/render`). This is the same endpoint Steam's own web
//! UI calls when scrolling market history — there is no official API for
//! this data. See `.mallet/features/steam-session-import/state.md` for the
//! full research behind the endpoint shape and the incremental-sync design.

use std::collections::{HashMap, HashSet};

use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

/// Steam Community Market history render endpoint. Authenticated via the
/// `steamLoginSecure` cookie, sent as a `Cookie:` header (no `sessionid`
/// needed for this GET-only endpoint).
const HISTORY_URL: &str = "https://steamcommunity.com/market/myhistory/render/";
/// Matches the confirmed real-world max page size for this endpoint.
const PAGE_SIZE: i64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionAction {
    Bought,
    Sold,
}

/// `raw_date` starts as Steam's literal `"Sold: 18 Jul"`-style text from
/// parsing, but `commands::preview_steam_import` overwrites it in place with
/// an inferred `"YYYY-MM-DD"` once `date_infer::infer_years` has run — by
/// the time a `SteamTransaction` reaches the frontend or
/// `commands::commit_steam_import`, this field holds the resolved date, not
/// the original Steam text.
///
/// `price` is `None` only for the synthetic entries `reconcile` fabricates
/// for currently-held items with no matching purchase in history (gifted,
/// traded, or crafted) — every row parsed directly from history always has
/// a real price, since a row is skipped entirely (see `parse_results_html`)
/// if its price can't be parsed at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamTransaction {
    pub row_id: String,
    pub appid: i64,
    pub contextid: String,
    pub market_hash_name: String,
    /// The `market_listing_game_name` text Steam shows under the item name
    /// — for most games this is just the game's own name, but for Steam
    /// Trading Cards (appid 753) it's the specific card set's title (e.g.
    /// "Townsmen - A Kingdom Rebuilt Trading Card"), which varies per item
    /// even though the appid is constant. Confirmed against the real
    /// fixture. Empty for synthetic (unmatched-holding) entries whose game
    /// name couldn't be recovered from any transaction in history — see
    /// `reconcile::synthetic_entry`.
    pub game_name: String,
    pub price: Option<f64>,
    pub action: TransactionAction,
    pub raw_date: String,
}

#[derive(Debug, Deserialize)]
struct HistoryPageResponse {
    success: bool,
    total_count: i64,
    start: i64,
    assets: serde_json::Value,
    hovers: String,
    results_html: String,
}

/// GETs one page of `myhistory/render`, authenticated via the caller's Steam
/// session cookie.
async fn fetch_page(cookie: &str, start: i64) -> Result<HistoryPageResponse, String> {
    let url = reqwest::Url::parse_with_params(
        HISTORY_URL,
        &[
            ("query", ""),
            ("start", &start.to_string()),
            ("count", &PAGE_SIZE.to_string()),
        ],
    )
    .map_err(|e| format!("failed to build history URL: {e}"))?;

    let client = reqwest::Client::builder()
        .user_agent(crate::ua::PRIMARY)
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let response = client
        .get(url)
        .header("Cookie", format!("steamLoginSecure={cookie}"))
        .send()
        .await
        .map_err(|e| format!("request to Steam market history failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Steam market history request failed with status {}",
            response.status()
        ));
    }

    let body: HistoryPageResponse = response
        .json()
        .await
        .map_err(|e| format!("failed to parse Steam market history response: {e}"))?;

    if !body.success {
        return Err("Steam market history request reported success: false".to_string());
    }

    Ok(body)
}

/// Parses the `hovers` blob of `CreateItemHoverFromContainer(...)` call
/// strings into a `row_id -> (appid, contextid, assetid)` map. This is the
/// only link between a `results_html` row and its `assets` entry.
///
/// Entries whose id ends in `_image` are skipped — they duplicate the
/// `_name` entry for the same row (same appid/contextid/assetid, just a
/// second DOM hook for the item thumbnail).
fn parse_hovers(hovers: &str) -> HashMap<String, (String, String, String)> {
    let pattern = regex::Regex::new(
        r"CreateItemHoverFromContainer\( g_rgAssets, '([^']+)', (\d+), '(\d+)', '(\d+)', 0 \);",
    )
    .expect("hover regex is a valid static pattern");

    let mut map = HashMap::new();
    for captures in pattern.captures_iter(hovers) {
        let id = captures[1].to_string();
        if id.ends_with("_image") {
            continue;
        }
        let appid = captures[2].to_string();
        let contextid = captures[3].to_string();
        let assetid = captures[4].to_string();
        map.insert(id, (appid, contextid, assetid));
    }
    map
}

/// Parses the `results_html` fragment into structured transactions,
/// cross-referencing `hovers` (for the appid/contextid/assetid triple) and
/// `assets` (for the canonical `market_hash_name`) for each row.
fn parse_results_html(
    html: &str,
    hovers: &HashMap<String, (String, String, String)>,
    assets: &serde_json::Value,
) -> Vec<SteamTransaction> {
    let fragment = Html::parse_fragment(html);
    let row_selector =
        Selector::parse(".market_listing_row").expect("static selector is valid");
    let gainorloss_selector =
        Selector::parse(".market_listing_gainorloss").expect("static selector is valid");
    let price_selector =
        Selector::parse(".market_listing_price").expect("static selector is valid");
    let date_selector = Selector::parse(".market_listing_listed_date_combined")
        .expect("static selector is valid");
    let game_name_selector =
        Selector::parse(".market_listing_game_name").expect("static selector is valid");

    let mut transactions = Vec::new();

    for row in fragment.select(&row_selector) {
        let Some(row_id) = row.value().attr("id") else {
            continue;
        };
        let row_id = row_id.to_string();

        // Confirmed against the real fixture (see
        // src-tauri/tests/fixtures/sample-market-history-response.json):
        // `market_listing_gainorloss` is "-" for a row whose
        // `market_listing_listed_date_combined` text is "Sold: ...". Since
        // the only two transaction kinds are Bought/Sold, "+" is the
        // complementary case (a purchase).
        let Some(gainorloss) = row.select(&gainorloss_selector).next() else {
            continue;
        };
        let action = match gainorloss.text().collect::<String>().trim() {
            "-" => TransactionAction::Sold,
            "+" => TransactionAction::Bought,
            _ => continue,
        };

        let Some(price) = row
            .select(&price_selector)
            .next()
            .and_then(|el| crate::currency::parse_amount(&el.text().collect::<String>()))
        else {
            continue;
        };

        let raw_date = row
            .select(&date_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let game_name = row
            .select(&game_name_selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let hover_key = format!("{row_id}_name");
        let Some((appid, contextid, assetid)) = hovers.get(&hover_key) else {
            continue;
        };
        let Ok(appid_num) = appid.parse::<i64>() else {
            continue;
        };

        let Some(market_hash_name) = assets
            .get(appid)
            .and_then(|by_context| by_context.get(contextid))
            .and_then(|by_asset| by_asset.get(assetid))
            .and_then(|asset| asset.get("market_hash_name"))
            .and_then(|name| name.as_str())
        else {
            continue;
        };

        transactions.push(SteamTransaction {
            row_id,
            appid: appid_num,
            contextid: contextid.clone(),
            market_hash_name: market_hash_name.to_string(),
            game_name,
            price: Some(price),
            action,
            raw_date,
        });
    }

    transactions
}

/// Paginates through the full Steam market history, skipping any `row_id`
/// already present in `already_imported`, until the endpoint's own
/// `total_count` is reached. Sleeps ~1s between requests, mirroring
/// `steam::refresh_all_prices`'s existing rate-limit courtesy.
///
/// Deliberately does NOT stop early at the first already-known row, even
/// though history comes back in reverse-chronological order (which would
/// make that a valid optimization *if* `already_imported` always reflected
/// everything evaluated so far). It doesn't: the Add/Import screen lets a
/// user commit only a filtered subset of a preview (e.g. "Rust" only, see
/// add.js's game filter), so a row can be legitimately new-to-the-vault
/// while chronologically *older* than an already-committed row from a
/// different game. Confirmed live: after a Rust-only commit, an early-stop
/// walk hit that Rust game's most recent (and therefore earliest-scanned)
/// row within the first page and silently discarded every un-imported
/// transaction beyond it — including hundreds of legitimately new rows from
/// other games — which is exactly why "fetch for all games" surfaced
/// nothing to commit. A full walk costs a few extra seconds (one request
/// per ~500 rows) but is the only version of this that's actually correct.
pub async fn sync_history(
    cookie: &str,
    already_imported: &HashSet<String>,
    progress: Option<&crate::progress::ProgressSender>,
) -> Result<Vec<SteamTransaction>, String> {
    let mut collected = Vec::new();
    let mut start = 0i64;

    loop {
        let page = fetch_page(cookie, start).await?;
        let hovers = parse_hovers(&page.hovers);
        let transactions = parse_results_html(&page.results_html, &hovers, &page.assets);
        // Visible in a console (e.g. a debug build); a no-op in the release
        // GUI app, which has no attached console to receive it.
        eprintln!(
            "steam_history: page start={start} total_count={} hovers_entries={} rows_parsed={}",
            page.total_count,
            hovers.len(),
            transactions.len(),
        );

        for transaction in transactions {
            if !already_imported.contains(&transaction.row_id) {
                collected.push(transaction);
            }
        }

        let scanned = (start + PAGE_SIZE).min(page.total_count);
        crate::progress::report(
            progress,
            format!("Scanned {scanned} of {} market history transactions...", page.total_count),
        );

        start += PAGE_SIZE;
        if start >= page.total_count {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    }

    Ok(collected)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/sample-market-history-response.json");

    #[derive(Deserialize)]
    struct FixtureResponse {
        assets: serde_json::Value,
        hovers: String,
        results_html: String,
    }

    fn load_fixture() -> FixtureResponse {
        serde_json::from_str(FIXTURE).expect("fixture is valid JSON matching HistoryPageResponse")
    }

    #[test]
    fn parse_hovers_maps_known_row_to_appid_contextid_assetid() {
        let fixture = load_fixture();
        let hovers = parse_hovers(&fixture.hovers);

        let entry = hovers
            .get("history_row_649197877515299133_649197877515299134_name")
            .expect("known row id from the fixture should be present");

        assert_eq!(
            entry,
            &("753".to_string(), "6".to_string(), "29992909246".to_string())
        );
    }

    #[test]
    fn parse_hovers_skips_image_duplicate_entries() {
        let fixture = load_fixture();
        let hovers = parse_hovers(&fixture.hovers);

        assert!(!hovers
            .keys()
            .any(|id| id.ends_with("_image")));
    }

    #[test]
    fn parse_results_html_produces_expected_first_transaction() {
        let fixture = load_fixture();
        let hovers = parse_hovers(&fixture.hovers);
        let transactions = parse_results_html(&fixture.results_html, &hovers, &fixture.assets);

        assert!(!transactions.is_empty(), "expected at least one parsed transaction");

        let first = &transactions[0];
        assert_eq!(first.row_id, "history_row_649197877515299133_649197877515299134");
        assert_eq!(first.appid, 753);
        assert_eq!(first.market_hash_name, "938380-Townie");
        assert_eq!(first.game_name, "Townsmen - A Kingdom Rebuilt Trading Card");
        assert_eq!(first.price, Some(0.03));
        // Confirmed against the fixture: this row's `market_listing_gainorloss`
        // text is "-" and its `market_listing_listed_date_combined` text is
        // "Sold: 18 Jul" — so "-" maps to Sold.
        assert!(matches!(first.action, TransactionAction::Sold));
        assert_eq!(first.raw_date, "Sold: 18 Jul");
    }

    #[test]
    fn parse_results_html_parses_every_row_in_the_fixture() {
        let fixture = load_fixture();
        let hovers = parse_hovers(&fixture.hovers);
        let transactions = parse_results_html(&fixture.results_html, &hovers, &fixture.assets);

        // The fixture's results_html contains 10 market_listing_row divs, all
        // sales ("-" / "Sold: ..."). There is no "+"/Bought example in this
        // real sample, so the Bought branch is exercised only by the
        // synthetic test below, not this fixture-derived one.
        assert_eq!(transactions.len(), 10);
        assert!(transactions
            .iter()
            .all(|t| matches!(t.action, TransactionAction::Sold)));
    }

    #[test]
    fn parse_results_html_maps_plus_sign_to_bought() {
        // The real fixture only contains "Sold" ("-") rows, so this
        // synthetic case documents and pins the complementary "+" -> Bought
        // mapping implied by the two-valued TransactionAction enum.
        let html = r#"
            <div class="market_listing_row" id="history_row_1_2">
                <div class="market_listing_gainorloss">+</div>
                <span class="market_listing_price">£1.23</span>
                <div class="market_listing_listed_date_combined">Purchased: 1 Jan</div>
            </div>
        "#;
        let mut hovers = HashMap::new();
        hovers.insert(
            "history_row_1_2_name".to_string(),
            ("753".to_string(), "6".to_string(), "1".to_string()),
        );
        let assets = serde_json::json!({
            "753": { "6": { "1": { "market_hash_name": "Some Item" } } }
        });

        let transactions = parse_results_html(html, &hovers, &assets);
        assert_eq!(transactions.len(), 1);
        assert!(matches!(transactions[0].action, TransactionAction::Bought));
    }
}
