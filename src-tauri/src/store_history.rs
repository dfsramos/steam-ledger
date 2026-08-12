//! Authenticated fetch + parse of Steam's in-game *store* purchase history
//! (`store.steampowered.com/account/history/`) — distinct from
//! `steam_history.rs`, which covers Community *Market* trades. See
//! `.mallet/features/steam-store-purchase-import/state.md` for the full live
//! investigation this module is built against, including the real
//! `AjaxLoadMoreHistory` response shape captured via
//! `src/bin/steam_import_debug.rs --test-store-pagination`.

use std::collections::HashSet;

use scraper::{ElementRef, Html, Selector};
use serde::Deserialize;

const HISTORY_URL: &str = "https://store.steampowered.com/account/history/";
const LOAD_MORE_URL: &str = "https://store.steampowered.com/account/AjaxLoadMoreHistory/";

/// A single named purchase extracted from one `wallet_history_table` row.
/// `item_names` holds 2+ entries for a "pack"-style checkout that bundled
/// several named things together in one row — Steam never exposes which
/// individual inventory skins those packs actually contain (confirmed live,
/// see `state.md`), so multi-item rows are surfaced as flagged/informational
/// only by downstream reconciliation (task 03), never auto-priced.
#[derive(Debug, Clone, PartialEq)]
pub struct StorePurchase {
    /// Derived from the `transid=` query param on the row's `onclick`
    /// help-center link — stable across re-fetches, so safe to use as the
    /// dedup key for incremental sync.
    pub row_id: String,
    /// The `appid=` query param on the row's help-center link, when present.
    /// Confirmed live that this is **not** present on every purchase row:
    /// only `HelpWithItemPurchase`-style links (typical "In-Game Purchase"
    /// rows) carry it — `HelpWithMyPurchase` (wallet-credit top-ups) and
    /// `HelpWithTransaction` (e.g. the real "Rust Warhammer Pack" purchase
    /// in the fixture) never include an `appid` param at all, even for a
    /// genuine single-item purchase. Reconciliation (task 03) treats
    /// `None` as "unknown, don't filter on it" rather than a hard mismatch,
    /// since being strict here would silently drop real matches that
    /// state.md's live investigation already confirmed are name-unique.
    pub appid: Option<i64>,
    /// Steam's own literal date text (e.g. "18 Jun, 2026") — already
    /// includes the year, unlike the Market history endpoint's "Sold: 18
    /// Jul" text, so (unlike `steam_history::SteamTransaction`) no
    /// `date_infer` pass is needed here.
    pub date: String,
    /// The leading, non-`wth_payment` div's text in the row's `wht_items`
    /// cell (almost always the game's name, e.g. "Rust"). Empty when a row
    /// has no such div — a single named purchase with no separate game-name
    /// wrapper (e.g. a "Warhammer Pack" bought standalone).
    pub game_name: String,
    pub item_names: Vec<String>,
    pub total_price: Option<f64>,
    /// The currency symbol as it literally appears in `wht_total` (e.g.
    /// "€", "£") — not an ISO code, mirroring how the rest of this codebase
    /// treats currency (see `currency::parse_amount`).
    pub currency: String,
}

const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Reformats `StorePurchase::date`'s literal `"18 Jun, 2026"`-style text
/// into `"YYYY-MM-DD"`, the format `items.date_purchased` is stored in
/// everywhere else in this app (`date_infer::infer_years` produces the same
/// shape for Market history). Unlike Market history's year-less dates, the
/// year is already present here (see `StorePurchase::date`'s doc comment),
/// so this is pure reformatting, not inference — returns `None` rather than
/// guessing if the text doesn't match the expected shape.
pub fn parse_store_date(raw: &str) -> Option<String> {
    let (day_month, year) = raw.trim().split_once(',')?;
    let year: u32 = year.trim().parse().ok()?;
    let mut tokens = day_month.split_whitespace();
    let day: u32 = tokens.next()?.parse().ok()?;
    let month_name = tokens.next()?;
    if tokens.next().is_some() {
        return None;
    }
    let month = MONTHS.iter().position(|m| *m == month_name)? as u32 + 1;
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Deserializes the confirmed-live `AjaxLoadMoreHistory` JSON response
/// shape: `{"html": "<tr>...</tr>...", "cursor": {...}}` when there's more
/// to paginate, with the `cursor` key entirely ABSENT (not an explicit
/// `null`) once the end is reached — captured live via
/// `steam_import_debug.rs --test-store-pagination` (see
/// `state.md`'s Blockers section for the raw bytes observed). `cursor` is
/// kept as an opaque `serde_json::Value` rather than a fixed struct: it's
/// just Steam's own `g_historyCursor`-shaped object round-tripped back
/// verbatim on the next request, and the exact fields inside it
/// (`wallet_txnid`/`timestamp_newest`/`balance`/`currency`, per the initial
/// page's embedded JS) are never inspected by this code, only forwarded —
/// pinning them into a struct would only create a way for this to break if
/// Steam adds a field.
#[derive(Debug, Deserialize)]
struct LoadMoreResponse {
    html: String,
    #[serde(default)]
    cursor: Option<serde_json::Value>,
}

/// Parses one page of the store history table (either the full initial page
/// fetched from `HISTORY_URL`, or the bare `<tr>` fragment returned by
/// `AjaxLoadMoreHistory`'s `html` field) into `StorePurchase`s, excluding
/// wallet-credit top-ups and Market Transaction rows (both handled
/// elsewhere: top-ups aren't purchases at all, and Market rows are already
/// covered by `steam_history.rs`'s import path).
///
/// A bare `<tr>...</tr>` fragment (no enclosing `<table>`/`<tbody>`) hits an
/// HTML5 parsing pitfall: per the spec, a `<tr>` start tag encountered
/// before the parser has ever entered "in table" insertion mode (which only
/// happens after seeing a literal `<table>` token) is a parse error whose
/// token is silently ignored — so parsing such a fragment directly would
/// silently drop every row. Confirmed the real initial page already embeds
/// its own `<table class="wallet_history_table">`, so only fragments
/// lacking one need the synthetic wrapper.
pub fn parse_history_html(html: &str) -> Vec<StorePurchase> {
    let document = if html.contains("<table") {
        Html::parse_document(html)
    } else {
        Html::parse_fragment(&format!("<table><tbody>{html}</tbody></table>"))
    };

    let row_selector = Selector::parse("tr.wallet_table_row").expect("static selector is valid");
    let date_selector = Selector::parse("td.wht_date").expect("static selector is valid");
    let items_selector = Selector::parse("td.wht_items").expect("static selector is valid");
    let type_selector = Selector::parse("td.wht_type").expect("static selector is valid");
    let total_selector = Selector::parse("td.wht_total").expect("static selector is valid");
    let div_selector = Selector::parse("div").expect("static selector is valid");

    let transid_re = regex::Regex::new(r"transid=(\d+)").expect("valid static regex");
    let appid_re = regex::Regex::new(r"appid=(\d+)").expect("valid static regex");
    let wallet_credit_re =
        regex::Regex::new(r"^Purchased .+ Wallet Credit$").expect("valid static regex");
    let market_type_re =
        regex::Regex::new(r"^(Market Transaction|\d+ Market Transactions)$").expect("valid static regex");

    let mut purchases = Vec::new();

    for row in document.select(&row_selector) {
        let Some(onclick) = row.value().attr("onclick") else {
            continue;
        };
        let Some(row_id) = transid_re.captures(onclick).map(|c| c[1].to_string()) else {
            // Market Transaction rows link to steamcommunity.com/market/
            // with no transid at all — excluded here too, redundantly with
            // the wht_type check below, since there's nothing to dedup on.
            continue;
        };
        let appid = appid_re
            .captures(onclick)
            .and_then(|c| c[1].parse::<i64>().ok());

        let Some(type_cell) = row.select(&type_selector).next() else {
            continue;
        };
        let type_label = type_cell
            .select(&div_selector)
            .find(|d| !has_class(d, "wth_payment"))
            .map(text_of)
            .unwrap_or_else(|| text_of(type_cell));
        if market_type_re.is_match(&type_label) {
            continue;
        }

        let Some(items_cell) = row.select(&items_selector).next() else {
            continue;
        };
        let item_divs: Vec<ElementRef> = items_cell.select(&div_selector).collect();
        let payment_divs: Vec<&ElementRef> =
            item_divs.iter().filter(|d| has_class(d, "wth_payment")).collect();

        let (game_name, item_names) = if payment_divs.is_empty() {
            // Either genuinely bare text (wallet-credit rows, Market rows)
            // or a single un-wrapped/div-wrapped name with no separate
            // game-name div (e.g. "Rust Warhammer Pack").
            let text = text_of(items_cell);
            if wallet_credit_re.is_match(&text) || text.is_empty() {
                continue;
            }
            (String::new(), vec![text])
        } else {
            let game_name = item_divs
                .iter()
                .find(|d| !has_class(d, "wth_payment"))
                .map(|d| text_of(*d))
                .unwrap_or_default();
            let item_names: Vec<String> = payment_divs.iter().map(|d| text_of(**d)).collect();
            (game_name, item_names)
        };

        let date = row.select(&date_selector).next().map(text_of).unwrap_or_default();

        let total_text = row
            .select(&total_selector)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();
        let total_price = crate::currency::parse_amount(&total_text);
        let currency = extract_currency(&total_text);

        purchases.push(StorePurchase {
            row_id,
            appid,
            date,
            game_name,
            item_names,
            total_price,
            currency,
        });
    }

    purchases
}

fn has_class(el: &ElementRef, class: &str) -> bool {
    el.value().has_class(class, scraper::CaseSensitivity::CaseSensitive)
}

fn text_of(el: ElementRef) -> String {
    el.text().collect::<String>().trim().to_string()
}

/// Pulls the currency symbol out of a `wht_total`-style string by
/// discarding every digit/separator/whitespace character — e.g. "25,17€"
/// -> "€". Mirrors `currency::parse_amount`'s own symbol-agnostic parsing
/// rather than hardcoding a currency list.
fn extract_currency(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_ascii_digit() && *c != ',' && *c != '.' && !c.is_whitespace())
        .collect()
}

/// GETs the initial store history page, authenticated via the caller's
/// store-scoped Steam session cookie.
async fn fetch_initial_page(client: &reqwest::Client, cookie: &str) -> Result<String, String> {
    let response = client
        .get(HISTORY_URL)
        .header("Cookie", format!("steamLoginSecure={cookie}"))
        .send()
        .await
        .map_err(|e| format!("request to Steam store history failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Steam store history request failed with status {}",
            response.status()
        ));
    }

    response
        .text()
        .await
        .map_err(|e| format!("failed to read Steam store history response body: {e}"))
}

/// Reads the initial page's embedded `g_historyCursor = {...};` JS global —
/// itself a JSON object literal (double-quoted keys), so it parses directly
/// as `serde_json::Value` — and `g_sessionID = "...";`, needed to
/// authenticate the `AjaxLoadMoreHistory` POST (confirmed live: the request
/// is rejected with `{"success":21}` — Steam's `EResult::NotLoggedOn` —
/// unless a `sessionid` cookie matching the posted `sessionid` form field is
/// also sent).
fn extract_initial_cursor_and_session(html: &str) -> Result<(serde_json::Value, String), String> {
    let cursor_re =
        regex::Regex::new(r"g_historyCursor\s*=\s*(\{.*?\});").expect("valid static regex");
    let cursor_text = cursor_re
        .captures(html)
        .map(|c| c[1].to_string())
        .ok_or_else(|| "could not find g_historyCursor on the store history page".to_string())?;
    let cursor: serde_json::Value = serde_json::from_str(&cursor_text)
        .map_err(|e| format!("g_historyCursor was not valid JSON: {e}"))?;

    let session_re = regex::Regex::new(r#"g_sessionID\s*=\s*"([^"]+)""#).expect("valid static regex");
    let session_id = session_re
        .captures(html)
        .map(|c| c[1].to_string())
        .ok_or_else(|| "could not find g_sessionID on the store history page".to_string())?;

    Ok((cursor, session_id))
}

/// Flattens the cursor object into jQuery's own bracket-notation form
/// encoding (`cursor[key]=value` per top-level field) — the real client-side
/// JS calls `$J.ajax({ data: { cursor: g_historyCursor, sessionid: ... } })`
/// with `cursor` as a plain object, and jQuery's default (non-`traditional`)
/// `$.param()` serializes a nested object exactly this way, NOT as one
/// JSON-stringified field. Confirmed live (2026-08-12): the previous
/// single-JSON-string encoding got a real `HTTP 200 {"html":""}` response
/// every time — indistinguishable from "no more pages" — for an account a
/// real browser can page back well over a year further on, via the exact
/// same endpoint. `cursor` is kept as an opaque `serde_json::Value` (see its
/// own doc comment), so this flattens whatever top-level keys are present
/// rather than hardcoding field names.
pub fn cursor_form_fields(cursor: &serde_json::Value) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    if let serde_json::Value::Object(map) = cursor {
        for (key, value) in map {
            let value_text = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            };
            fields.push((format!("cursor[{key}]"), value_text));
        }
    }
    fields
}

/// POSTs one `AjaxLoadMoreHistory` page for the given cursor.
async fn fetch_more(
    client: &reqwest::Client,
    cookie: &str,
    session_id: &str,
    cursor: &serde_json::Value,
) -> Result<LoadMoreResponse, String> {
    let mut form = cursor_form_fields(cursor);
    form.push(("sessionid".to_string(), session_id.to_string()));

    let response = client
        .post(LOAD_MORE_URL)
        // The `sessionid` cookie (not just the form field) is required —
        // see `extract_initial_cursor_and_session`'s doc comment.
        .header(
            "Cookie",
            format!("steamLoginSecure={cookie}; sessionid={session_id}"),
        )
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("AjaxLoadMoreHistory request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "AjaxLoadMoreHistory request failed with status {}",
            response.status()
        ));
    }

    response
        .json::<LoadMoreResponse>()
        .await
        .map_err(|e| format!("failed to parse AjaxLoadMoreHistory response: {e}"))
}

/// Fetches and fully paginates the store purchase history, skipping any
/// `row_id` already present in `already_imported`. Deliberately does NOT
/// stop early at the first already-known row — see
/// `steam_history::sync_history`'s doc comment for why that assumption is
/// unsound whenever only a filtered subset of a previous preview was ever
/// committed. `already_imported` is currently always passed empty (see
/// `commands::preview_steam_import`), so this always walks the full store
/// history regardless; the parameter exists for a future incremental-sync
/// caller, and must stay correct under the same filtered-commit scenario
/// that broke `sync_history`.
pub async fn sync_store_history(
    cookie: &str,
    already_imported: &HashSet<String>,
    progress: Option<&crate::progress::ProgressSender>,
) -> Result<Vec<StorePurchase>, String> {
    let client = reqwest::Client::builder()
        .user_agent(crate::ua::PRIMARY)
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let initial_html = fetch_initial_page(&client, cookie).await?;
    let (mut cursor, session_id) = extract_initial_cursor_and_session(&initial_html)?;

    let mut collected = Vec::new();
    let mut page_purchases = parse_history_html(&initial_html);
    let mut page_index = 0u32;

    loop {
        eprintln!(
            "store_history: page {page_index} rows_parsed={}",
            page_purchases.len()
        );

        for purchase in page_purchases {
            if !already_imported.contains(&purchase.row_id) {
                collected.push(purchase);
            }
        }

        crate::progress::report(
            progress,
            format!("Scanned {} store purchase(s) so far...", collected.len()),
        );

        page_index += 1;
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

        let response = fetch_more(&client, cookie, &session_id, &cursor).await?;
        page_purchases = parse_history_html(&response.html);

        match response.cursor {
            Some(next_cursor) => cursor = next_cursor,
            None => {
                // The loop condition is only checked at the top, so run one
                // more iteration to fold in this final `page_purchases`
                // batch before stopping.
                for purchase in page_purchases {
                    if !already_imported.contains(&purchase.row_id) {
                        collected.push(purchase);
                    }
                }
                break;
            }
        }
    }

    Ok(collected)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/sample-store-history-response.html");

    // Regression test for a real bug: the cursor was previously sent as one
    // JSON-stringified form field, but the real client-side JS
    // ($J.ajax({data: {cursor: g_historyCursor, ...}})) has jQuery serialize
    // a nested object as bracket-notation fields — the old encoding got a
    // real HTTP 200 {"html":""} from Steam every time, indistinguishable
    // from "no more pages", silently truncating pagination for any account
    // with more than one page of store history. See state.md, 2026-08-12.
    #[test]
    fn cursor_form_fields_uses_bracket_notation_matching_jquerys_param_serialization() {
        let cursor: serde_json::Value = serde_json::from_str(
            r#"{"wallet_txnid":"89623195141","timestamp_newest":1754590453,"balance":"1898","currency":3}"#,
        )
        .expect("valid json");
        let mut fields = cursor_form_fields(&cursor);
        fields.sort();
        assert_eq!(
            fields,
            vec![
                ("cursor[balance]".to_string(), "1898".to_string()),
                ("cursor[currency]".to_string(), "3".to_string()),
                ("cursor[timestamp_newest]".to_string(), "1754590453".to_string()),
                ("cursor[wallet_txnid]".to_string(), "89623195141".to_string()),
            ]
        );
    }

    #[test]
    fn parse_store_date_reformats_real_observed_shapes() {
        // Real values captured live against the actual account (see this
        // task's live-verification notes) — single- and double-digit days.
        assert_eq!(parse_store_date("18 Jun, 2026"), Some("2026-06-18".to_string()));
        assert_eq!(parse_store_date("6 Aug, 2026"), Some("2026-08-06".to_string()));
        assert_eq!(parse_store_date("1 Jan, 2026"), Some("2026-01-01".to_string()));
    }

    #[test]
    fn parse_store_date_rejects_unrecognised_shapes() {
        assert_eq!(parse_store_date(""), None);
        assert_eq!(parse_store_date("not a date"), None);
        assert_eq!(parse_store_date("18 Jul"), None, "no year present — must not guess one");
        assert_eq!(parse_store_date("18 Xyz, 2026"), None, "not a recognised month abbreviation");
    }

    #[test]
    fn parse_history_html_excludes_wallet_credit_rows() {
        let purchases = parse_history_html(FIXTURE);
        assert!(
            !purchases.iter().any(|p| p.row_id == "245034690245707685"),
            "the 'Purchased 24,42€ Wallet Credit' row must be excluded"
        );
    }

    #[test]
    fn parse_history_html_excludes_market_transaction_rows() {
        let purchases = parse_history_html(FIXTURE);
        // The fixture's two Market-Transaction-typed rows link to
        // steamcommunity.com/market/ with no transid at all, so they can't
        // produce a row_id — assert indirectly via the exact expected count
        // (3 real purchases out of 6 total fixture rows: pack, single-item,
        // and the bare "Rust Warhammer Pack" row).
        assert_eq!(purchases.len(), 3);
    }

    #[test]
    fn parse_history_html_parses_single_item_row() {
        let purchases = parse_history_html(FIXTURE);
        let bamboo = purchases
            .iter()
            .find(|p| p.row_id == "435307335345635971")
            .expect("Bamboo Cage Fridge row should be parsed");

        assert_eq!(bamboo.date, "18 Jun, 2026");
        assert_eq!(bamboo.game_name, "Rust");
        assert_eq!(bamboo.item_names, vec!["Bamboo Cage Fridge".to_string()]);
        assert_eq!(bamboo.total_price, Some(2.65));
        assert_eq!(bamboo.currency, "€");
        assert_eq!(bamboo.appid, Some(252490));
    }

    #[test]
    fn parse_history_html_parses_multi_item_row() {
        let purchases = parse_history_html(FIXTURE);
        let pack = purchases
            .iter()
            .find(|p| p.row_id == "245034690245686150")
            .expect("the 3-item pack row should be parsed");

        assert_eq!(pack.date, "6 Aug, 2026");
        assert_eq!(pack.game_name, "Rust");
        assert_eq!(
            pack.item_names,
            vec![
                "Industrial Decor Pack".to_string(),
                "Cargo Heli Small Backpack".to_string(),
                "Bar Games Pack".to_string(),
            ]
        );
        assert_eq!(pack.total_price, Some(25.17));
        assert_eq!(pack.currency, "€");
        assert_eq!(pack.appid, Some(252490));
    }

    #[test]
    fn parse_history_html_parses_bare_single_name_row_with_no_game_name_div() {
        let purchases = parse_history_html(FIXTURE);
        let warhammer = purchases
            .iter()
            .find(|p| p.row_id == "127925249075125108")
            .expect("the bare 'Rust Warhammer Pack' row should be parsed");

        assert_eq!(warhammer.game_name, "");
        assert_eq!(warhammer.item_names, vec!["Rust Warhammer Pack".to_string()]);
        assert_eq!(warhammer.total_price, Some(10.23));
        assert_eq!(warhammer.currency, "€");
        // This row's help-center link is a `HelpWithTransaction` URL with no
        // `appid=` param at all (confirmed live) — even though it's a real,
        // genuine single-item purchase. See the `appid` field doc comment.
        assert_eq!(warhammer.appid, None);
    }

    #[test]
    fn parse_history_html_handles_a_bare_tr_fragment_with_no_table_wrapper() {
        // Mirrors the AjaxLoadMoreHistory response's `html` field, which
        // (per the doc comment on `parse_history_html`) is just `<tr>...`
        // with no enclosing table/tbody — a synthetic case, since the one
        // real AjaxLoadMoreHistory response captured live during this
        // task's investigation had no further pages to return (empty
        // `html`, see state.md's Blockers section).
        let fragment = r#"
            <tr class="wallet_table_row wallet_table_row_amt_change" onclick="location.href='https://help.steampowered.com/en/wizard/HelpWithItemPurchase?transid=999&appid=252490'">
                <td class="wht_date">1 Jan, 2026</td>
                <td class="wht_items "><div>Rust</div><div class="wth_payment">Some Skin</div></td>
                <td class="wht_type "><div>In-Game Purchase</div></td>
                <td class="wht_total ">1,00€</td>
            </tr>
        "#;

        let purchases = parse_history_html(fragment);
        assert_eq!(purchases.len(), 1);
        assert_eq!(purchases[0].row_id, "999");
        assert_eq!(purchases[0].game_name, "Rust");
        assert_eq!(purchases[0].item_names, vec!["Some Skin".to_string()]);
        assert_eq!(purchases[0].total_price, Some(1.0));
        assert_eq!(purchases[0].appid, Some(252490));
    }

    #[test]
    fn extract_currency_strips_digits_and_separators() {
        assert_eq!(extract_currency("25,17€"), "€");
        assert_eq!(extract_currency("£1,234.56"), "£");
    }

    #[test]
    fn extract_initial_cursor_and_session_reads_real_js_globals() {
        let html = r#"
            <script>
                g_historyCursor = {"wallet_txnid":"89623195141","timestamp_newest":1754590453,"balance":"1898","currency":3};
                g_sessionID = "70a903b4111d2c3844681cf0";
            </script>
        "#;
        let (cursor, session_id) = extract_initial_cursor_and_session(html).expect("should parse");
        assert_eq!(cursor["wallet_txnid"], "89623195141");
        assert_eq!(session_id, "70a903b4111d2c3844681cf0");
    }
}
