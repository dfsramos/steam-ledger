//! Fetches and parses Steam's authenticated purchase-detail "help wizard"
//! page — the only place Steam exposes a per-item price breakdown for a
//! multi-item ("pack") store purchase; the wallet-history table itself
//! (`store_history.rs`) only ever shows the pack's name and total price.
//! Confirmed live (2026-08-12) that this needs a THIRD, independently-scoped
//! session cookie, distinct from both the Market and Store cookies already
//! used elsewhere in this feature — both of those redirect to
//! `help.steampowered.com`'s own `/en/login` page. See `credentials.rs`'s
//! `HELP_COOKIE_USER` and this feature's `state.md`.

use scraper::{Html, Selector};

const HELP_WIZARD_URL: &str = "https://help.steampowered.com/en/wizard/HelpWithItemPurchase";

/// One named item's price within a multi-item purchase, tax-inclusive — see
/// `parse_pack_breakdown`'s doc comment for how tax is allocated.
#[derive(Debug, Clone, PartialEq)]
pub struct PackItemPrice {
    pub name: String,
    pub price: f64,
}

/// Fetches the help-wizard page for one purchase (`transid` is the same
/// value as `StorePurchase::row_id`) and parses its per-item price
/// breakdown. Returns `None` (not an error) whenever the page can't be read
/// as a genuine purchase-detail page — an expired/missing help cookie
/// redirects to a login page with none of the expected markup, which this
/// treats the same as "not available" rather than a hard failure that would
/// abort the whole import: this data is a refinement on top of packs
/// already being usably flagged, not something the rest of the import
/// should ever depend on.
pub async fn fetch_pack_breakdown(cookie: &str, transid: &str, appid: i64) -> Option<Vec<PackItemPrice>> {
    let client = reqwest::Client::builder().user_agent(crate::ua::PRIMARY).build().ok()?;

    let response = client
        .get(HELP_WIZARD_URL)
        .query(&[("transid", transid), ("appid", &appid.to_string())])
        .header("Cookie", format!("steamLoginSecure={cookie}"))
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }
    let html = response.text().await.ok()?;
    parse_pack_breakdown(&html)
}

/// Parses the authenticated purchase-detail page's per-item price
/// breakdown, allocating tax proportionally across items so the sum of
/// returned prices always reconstructs the purchase's real `Total` (up to
/// rounding). This matches how every other `price_paid` in this app already
/// represents what was actually paid *including* tax (see
/// `StorePurchase::total_price`, itself sourced from Steam's own
/// tax-inclusive `wht_total` column) — using each item's pre-tax subtotal
/// alone would silently under-represent cost basis for pack items
/// specifically, inconsistent with every other price in the app.
pub fn parse_pack_breakdown(html: &str) -> Option<Vec<PackItemPrice>> {
    let document = Html::parse_document(html);
    let item_row_selector = Selector::parse(".purchase_line_items > div").expect("static selector is valid");
    let name_selector = Selector::parse(".purchase_detail_field").expect("static selector is valid");
    let price_selector = Selector::parse(".refund_value span").expect("static selector is valid");
    let total_row_selector = Selector::parse("table.purchase_totals tr").expect("static selector is valid");
    let header_selector = Selector::parse("td.purchase_total_header").expect("static selector is valid");
    let total_value_selector = Selector::parse("td.refund_value span").expect("static selector is valid");

    let mut items: Vec<(String, f64)> = Vec::new();
    for row in document.select(&item_row_selector) {
        let Some(name_el) = row.select(&name_selector).next() else { continue };
        let Some(price_el) = row.select(&price_selector).next() else { continue };
        let name = name_el.text().collect::<String>().trim().to_string();
        let Some(price) = crate::currency::parse_amount(&price_el.text().collect::<String>()) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        items.push((name, price));
    }

    if items.is_empty() {
        return None;
    }

    let mut subtotal: Option<f64> = None;
    let mut tax: Option<f64> = None;
    for row in document.select(&total_row_selector) {
        let Some(header) = row.select(&header_selector).next() else { continue };
        let label = header.text().collect::<String>();
        let label = label.trim();
        let Some(value_el) = row.select(&total_value_selector).next() else { continue };
        let Some(value) = crate::currency::parse_amount(&value_el.text().collect::<String>()) else {
            continue;
        };
        match label {
            "Subtotal" => subtotal = Some(value),
            "Tax" => tax = Some(value),
            _ => {}
        }
    }

    let subtotal = subtotal.unwrap_or_else(|| items.iter().map(|(_, p)| p).sum());
    let tax = tax.unwrap_or(0.0);

    if subtotal <= 0.0 {
        // Can't allocate proportionally against a zero/negative subtotal —
        // fall back to each item's raw pre-tax price rather than divide by
        // zero (shouldn't happen in practice: a real purchase always has a
        // positive subtotal).
        return Some(items.into_iter().map(|(name, price)| PackItemPrice { name, price }).collect());
    }

    Some(
        items
            .into_iter()
            .map(|(name, price)| {
                let allocated = price + tax * (price / subtotal);
                PackItemPrice { name, price: (allocated * 100.0).round() / 100.0 }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/sample-help-wizard-response.html");

    #[test]
    fn parses_the_real_two_item_pack_fixture_with_tax_allocated() {
        let items = parse_pack_breakdown(FIXTURE).expect("should parse");
        assert_eq!(
            items,
            vec![
                PackItemPrice { name: "Tropical Bed".to_string(), price: 1.75 },
                PackItemPrice { name: "Cargo Heli Bed".to_string(), price: 2.19 },
            ]
        );
        let sum: f64 = items.iter().map(|i| i.price).sum();
        assert!((sum - 3.94).abs() < 0.01, "allocated prices should sum back to the real Total");
    }

    #[test]
    fn returns_none_for_a_login_redirect_page() {
        let login_page = "<html><title>Steam Help</title><body>Sign in</body></html>";
        assert_eq!(parse_pack_breakdown(login_page), None);
    }

    #[test]
    fn falls_back_to_raw_price_when_subtotal_and_tax_are_missing() {
        let html = r#"<div class="purchase_line_items">
            <div><span class="purchase_detail_field">Solo Item</span> - <span class="refund_value"><span>5,00€</span></span></div>
        </div>"#;
        let items = parse_pack_breakdown(html).expect("should parse");
        assert_eq!(items, vec![PackItemPrice { name: "Solo Item".to_string(), price: 5.00 }]);
    }
}
