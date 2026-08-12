//! Reconciles Steam Inventory holdings (ground truth for what the user
//! currently owns) against Market history (source of purchase price/date).
//! Market history alone can't answer "what do I still own" — an item bought
//! then later resold via the market still shows up as a `Bought` row
//! forever, which is exactly the bug this fixes. See
//! `.mallet/features/steam-session-import/state.md` for the full design
//! reasoning.

use std::collections::HashMap;

use crate::steam_history::{SteamTransaction, TransactionAction};

/// For each `(appid, market_hash_name)` with a known held count (from
/// inventory), matches up to `held_count` of the most-recent `Bought`
/// transactions for that item as the probable acquisitions — price and date
/// come along with the match. `Bought` transactions in excess of the held
/// count (presumably resold since) are dropped entirely; this is what fixes
/// "importing items I no longer have". Held units beyond what history can
/// account for (gifted, traded, or crafted — never went through the market)
/// become synthetic entries with `price: None`, so nothing currently owned
/// goes missing from the import, flagged for the user to fill in manually —
/// *unless* `marketable` says the item isn't currently tradeable, in which
/// case it's skipped entirely (see the per-key check below for why a
/// history match always overrides this). `Sold` transactions pass through
/// unchanged — still informational-only.
pub fn reconcile_holdings_with_history(
    holdings: &HashMap<(i64, String), i64>,
    marketable: &HashMap<(i64, String), bool>,
    app_names: &HashMap<i64, String>,
    transactions: Vec<SteamTransaction>,
) -> Vec<SteamTransaction> {
    let mut sold: Vec<SteamTransaction> = Vec::new();
    let mut bought_by_key: HashMap<(i64, String), Vec<SteamTransaction>> = HashMap::new();
    // Captured before transactions are moved into bought_by_key/sold below,
    // so a synthetic (unmatched) entry can still show which app/game an
    // item is from even though it has no transaction of its own to read
    // that from directly.
    let mut game_name_by_key: HashMap<(i64, String), String> = HashMap::new();

    for t in transactions {
        let key = (t.appid, t.market_hash_name.clone());
        if !t.game_name.is_empty() {
            game_name_by_key.entry(key.clone()).or_insert_with(|| t.game_name.clone());
        }
        match t.action {
            TransactionAction::Sold => sold.push(t),
            TransactionAction::Bought => {
                bought_by_key.entry(key).or_default().push(t);
            }
        }
    }

    // Most-recent first within each group (raw_date is "YYYY-MM-DD" by the
    // time this runs, after date_infer — lexicographic sort is chronological).
    for group in bought_by_key.values_mut() {
        group.sort_by(|a, b| b.raw_date.cmp(&a.raw_date));
    }

    let mut result = Vec::new();

    for (key, held_count) in holdings {
        if *held_count <= 0 {
            continue;
        }
        let held_count = *held_count as usize;
        let bought = bought_by_key.remove(key);
        let matched = bought.as_deref().map_or(0, |b| b.len().min(held_count));

        if let Some(bought) = bought {
            for t in bought.into_iter().take(matched) {
                result.push(t);
            }
        }

        // Only the *unmatched* shortfall is gated on marketable status — a
        // unit that already matched a real purchase above is always kept
        // regardless, since Steam's temporary post-purchase/trade market
        // hold (commonly up to 7 days) reports the same marketable: false
        // as a genuinely non-tradeable item, and a hold doesn't erase that
        // you bought it. Absence from `marketable` (key not present)
        // defaults to permissive/included, not exclusion.
        if marketable.get(key).copied().unwrap_or(true) {
            // A gifted/traded/crafted item with zero transaction history at
            // all has no game name to recover from a transaction — fall
            // back to the resolved Steam store name for its appid
            // (`app_names`, populated via the public appdetails API — see
            // `appinfo.rs`), and only as an absolute last resort (that
            // lookup itself failing, e.g. no network) show the raw appid
            // rather than nothing.
            let game_name = game_name_by_key.get(key).cloned().unwrap_or_else(|| {
                app_names
                    .get(&key.0)
                    .cloned()
                    .unwrap_or_else(|| format!("appid {}", key.0))
            });
            for i in matched..held_count {
                result.push(synthetic_entry(key, i, game_name.clone()));
            }
        }
    }
    // Any remaining bought_by_key entries have no matching holding at all
    // (item fully resold, or a game/context not covered by the inventory
    // fetch) — intentionally dropped, not carried into `result`.

    result.extend(sold);
    result
}

/// A held item with no matching purchase in history. `row_id` is
/// deterministic (not random) so a repeat import doesn't create a fresh
/// duplicate for the same still-unmatched holding — it's stable as long as
/// the held count for this item doesn't change between syncs.
fn synthetic_entry((appid, market_hash_name): &(i64, String), index: usize, game_name: String) -> SteamTransaction {
    SteamTransaction {
        row_id: format!("unmatched:{appid}:{market_hash_name}:{index}"),
        appid: *appid,
        contextid: String::new(),
        market_hash_name: market_hash_name.clone(),
        game_name,
        price: None,
        action: TransactionAction::Bought,
        raw_date: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bought(appid: i64, name: &str, price: f64, date: &str, row_id: &str) -> SteamTransaction {
        SteamTransaction {
            row_id: row_id.to_string(),
            appid,
            contextid: "2".to_string(),
            market_hash_name: name.to_string(),
            game_name: "Test Game".to_string(),
            price: Some(price),
            action: TransactionAction::Bought,
            raw_date: date.to_string(),
        }
    }

    fn sold(appid: i64, name: &str, price: f64, date: &str, row_id: &str) -> SteamTransaction {
        SteamTransaction {
            action: TransactionAction::Sold,
            ..bought(appid, name, price, date, row_id)
        }
    }

    #[test]
    fn drops_purchases_beyond_the_held_count() {
        // Bought 2, sold 1, currently hold 1 — the OLDER purchase should be
        // dropped, the newer one kept. This is the exact bug being fixed:
        // without reconciliation both purchases would import.
        let transactions = vec![
            bought(252490, "Widget", 5.0, "2026-01-01", "old-buy"),
            bought(252490, "Widget", 6.0, "2026-06-01", "new-buy"),
            sold(252490, "Widget", 5.5, "2026-07-01", "the-sale"),
        ];
        let mut holdings = HashMap::new();
        holdings.insert((252490, "Widget".to_string()), 1);

        let result = reconcile_holdings_with_history(&holdings, &HashMap::new(), &HashMap::new(), transactions);

        let bought_rows: Vec<_> = result.iter().filter(|t| t.action == TransactionAction::Bought).collect();
        assert_eq!(bought_rows.len(), 1);
        assert_eq!(bought_rows[0].row_id, "new-buy");
        // The sale is still surfaced, informational.
        assert!(result.iter().any(|t| t.row_id == "the-sale"));
    }

    #[test]
    fn drops_all_purchases_when_nothing_is_currently_held() {
        let transactions = vec![bought(252490, "Widget", 5.0, "2026-01-01", "buy-1")];
        let holdings = HashMap::new(); // no holding at all for this item

        let result = reconcile_holdings_with_history(&holdings, &HashMap::new(), &HashMap::new(), transactions);
        assert!(result.is_empty());
    }

    #[test]
    fn fabricates_an_unknown_price_entry_when_held_exceeds_bought_history() {
        // Holds 2, but only 1 purchase on record — the other was presumably
        // gifted/traded/crafted.
        let transactions = vec![bought(252490, "Widget", 5.0, "2026-01-01", "buy-1")];
        let mut holdings = HashMap::new();
        holdings.insert((252490, "Widget".to_string()), 2);

        let result = reconcile_holdings_with_history(&holdings, &HashMap::new(), &HashMap::new(), transactions);
        let bought_rows: Vec<_> = result.iter().filter(|t| t.action == TransactionAction::Bought).collect();
        assert_eq!(bought_rows.len(), 2);
        assert!(bought_rows.iter().any(|t| t.row_id == "buy-1" && t.price == Some(5.0)));
        assert!(bought_rows.iter().any(|t| t.price.is_none()));
    }

    #[test]
    fn fully_synthetic_when_held_but_never_in_history_at_all() {
        let mut holdings = HashMap::new();
        holdings.insert((252490, "Gifted Item".to_string()), 1);

        let result = reconcile_holdings_with_history(&holdings, &HashMap::new(), &HashMap::new(), vec![]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].price, None);
        assert_eq!(result[0].market_hash_name, "Gifted Item");
        // No transaction anywhere to recover a game name from — falls back
        // to the numeric appid rather than leaving it blank.
        assert_eq!(result[0].game_name, "appid 252490");
    }

    #[test]
    fn synthetic_entry_recovers_game_name_from_a_sold_transaction_for_the_same_item() {
        // Held 2, only 1 purchase on record, but a Sold row for the SAME
        // item exists too — its game_name should still be usable for the
        // synthetic (unmatched) entry.
        let transactions = vec![
            bought(252490, "Widget", 5.0, "2026-01-01", "buy-1"),
            sold(252490, "Widget", 6.0, "2026-02-01", "unrelated-sale"),
        ];
        let mut holdings = HashMap::new();
        holdings.insert((252490, "Widget".to_string()), 2);

        let result = reconcile_holdings_with_history(&holdings, &HashMap::new(), &HashMap::new(), transactions);
        let synthetic = result.iter().find(|t| t.price.is_none()).expect("expected a synthetic entry");
        assert_eq!(synthetic.game_name, "Test Game");
    }

    #[test]
    fn matched_purchase_keeps_its_own_game_name() {
        let transactions = vec![bought(252490, "Widget", 5.0, "2026-01-01", "buy-1")];
        let mut holdings = HashMap::new();
        holdings.insert((252490, "Widget".to_string()), 1);

        let result = reconcile_holdings_with_history(&holdings, &HashMap::new(), &HashMap::new(), transactions);
        assert_eq!(result[0].game_name, "Test Game");
    }

    #[test]
    fn keeps_purchases_up_to_the_held_count_unmodified() {
        let transactions = vec![bought(252490, "Widget", 5.0, "2026-01-01", "buy-1")];
        let mut holdings = HashMap::new();
        holdings.insert((252490, "Widget".to_string()), 1);

        let result = reconcile_holdings_with_history(&holdings, &HashMap::new(), &HashMap::new(), transactions);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].row_id, "buy-1");
        assert_eq!(result[0].price, Some(5.0));
    }

    #[test]
    fn repeat_reconciliation_with_unchanged_holdings_produces_the_same_synthetic_row_id() {
        let mut holdings = HashMap::new();
        holdings.insert((252490, "Gifted Item".to_string()), 1);

        let first = reconcile_holdings_with_history(&holdings, &HashMap::new(), &HashMap::new(), vec![]);
        let second = reconcile_holdings_with_history(&holdings, &HashMap::new(), &HashMap::new(), vec![]);
        assert_eq!(first[0].row_id, second[0].row_id);
    }

    #[test]
    fn skips_a_synthetic_entry_for_a_non_marketable_item_with_no_purchase_record() {
        // Held but no history match and explicitly non-marketable — a
        // non-tradeable bundle/quest/pack item, not a real portfolio holding.
        let mut holdings = HashMap::new();
        holdings.insert((252490, "Bundle Wrapper".to_string()), 1);
        let mut marketable = HashMap::new();
        marketable.insert((252490, "Bundle Wrapper".to_string()), false);

        let result = reconcile_holdings_with_history(&holdings, &marketable, &HashMap::new(), vec![]);
        assert!(result.is_empty(), "a non-marketable item with no purchase record should be skipped entirely");
    }

    #[test]
    fn still_fabricates_a_synthetic_entry_for_a_marketable_item_with_no_purchase_record() {
        let mut holdings = HashMap::new();
        holdings.insert((252490, "Gifted Item".to_string()), 1);
        let mut marketable = HashMap::new();
        marketable.insert((252490, "Gifted Item".to_string()), true);

        let result = reconcile_holdings_with_history(&holdings, &marketable, &HashMap::new(), vec![]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].price, None);
    }

    #[test]
    fn a_matched_purchase_is_kept_even_if_the_item_is_currently_non_marketable() {
        // Models the exact caveat this feature must not break: an item
        // bought today is still inside Steam's temporary post-purchase
        // market hold (marketable: false) but was DEFINITELY just bought —
        // the history match must win regardless of the current flag.
        let transactions = vec![bought(252490, "Freshly Bought Skin", 12.0, "2026-08-11", "buy-today")];
        let mut holdings = HashMap::new();
        holdings.insert((252490, "Freshly Bought Skin".to_string()), 1);
        let mut marketable = HashMap::new();
        marketable.insert((252490, "Freshly Bought Skin".to_string()), false);

        let result = reconcile_holdings_with_history(&holdings, &marketable, &HashMap::new(), transactions);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].row_id, "buy-today");
        assert_eq!(result[0].price, Some(12.0));
    }
}
