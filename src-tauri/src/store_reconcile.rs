//! Matches single-item `StorePurchase` rows (from `store_history.rs`) against
//! existing vault items that are still unpriced (`price_paid = 0.0`) —
//! Market-import's "synthetic"/unmatched entries, see `reconcile.rs`'s
//! `synthetic_entry` for how those originate. Multi-item ("pack") rows are
//! never auto-priced (approved design, `state.md`, 2026-08-12: Steam exposes
//! a pack's name and total price but never which individual skins are
//! inside), so they always land in `unmatched` for the caller to surface as
//! flagged/informational only.

use crate::store_history::StorePurchase;

/// Matches single-item purchases to unpriced vault-item candidates by exact
/// (case-sensitive) name (scoped to the same `appid` when known — see
/// below), and returns ready-to-apply `(item_id, item_name, price, date)`
/// tuples plus every purchase that couldn't be matched.
///
/// `candidates` is `(item_id, name, appid)` for vault items currently at
/// `price_paid = 0.0`.
///
/// `StorePurchase::appid` (`store_history.rs`) is itself an `Option<i64>`:
/// confirmed live that Steam's store history row only sometimes exposes an
/// `appid=` query param on its help-center link — `HelpWithItemPurchase`
/// rows ("In-Game Purchase") carry it, but `HelpWithTransaction` rows do
/// not, even for a genuine single-item purchase (e.g. the real "Rust
/// Warhammer Pack" row captured in `state.md`'s live investigation). So the
/// appid check here is permissive when unknown: `Some(appid)` must equal the
/// candidate's `appid` to match, but `None` falls back to name-only
/// matching rather than being auto-rejected — consistent with this
/// feature's live-verified findings (`state.md`, 2026-08-12) that every
/// single-item purchase name checked matched exactly, byte-for-byte, to an
/// existing vault item (7/7), with no cross-game name collisions observed.
///
/// A name can match more than one still-unpriced candidate (duplicate item
/// names can exist, see `reconcile.rs`'s handling of the same situation) —
/// `candidates` is walked in the order given, and the first not-yet-consumed
/// match wins, so callers should pass candidates oldest-first (e.g. by
/// ascending `item_id`/insertion order) for stable, deterministic re-runs
/// that don't re-match the same store purchase to a different item on every
/// import.
pub fn match_store_purchases(
    purchases: &[StorePurchase],
    candidates: &[(i64, String, i64)],
) -> (Vec<(i64, String, f64, String)>, Vec<StorePurchase>) {
    let mut consumed: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();

    for purchase in purchases {
        let is_single_named = purchase.item_names.len() == 1;
        let found = if is_single_named {
            let name = &purchase.item_names[0];
            candidates.iter().find(|(item_id, candidate_name, candidate_appid)| {
                candidate_name == name
                    && !consumed.contains(item_id)
                    && purchase.appid.is_none_or(|appid| appid == *candidate_appid)
            })
        } else {
            None
        };

        match (is_single_named, found, purchase.total_price) {
            (true, Some((item_id, candidate_name, _appid)), Some(price)) => {
                consumed.insert(*item_id);
                matched.push((*item_id, candidate_name.clone(), price, purchase.date.clone()));
            }
            _ => unmatched.push(purchase.clone()),
        }
    }

    (matched, unmatched)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn purchase(item_names: Vec<&str>, total_price: Option<f64>, date: &str, row_id: &str) -> StorePurchase {
        StorePurchase {
            row_id: row_id.to_string(),
            appid: Some(252490),
            date: date.to_string(),
            game_name: "Rust".to_string(),
            item_names: item_names.into_iter().map(str::to_string).collect(),
            total_price,
            currency: "€".to_string(),
        }
    }

    #[test]
    fn matches_a_single_item_purchase_to_its_candidate() {
        let purchases = vec![purchase(vec!["Bamboo Cage Fridge"], Some(2.65), "18 Jun, 2026", "row-1")];
        let candidates = vec![(672, "Bamboo Cage Fridge".to_string(), 252490)];

        let (matched, unmatched) = match_store_purchases(&purchases, &candidates);

        assert_eq!(matched, vec![(672, "Bamboo Cage Fridge".to_string(), 2.65, "18 Jun, 2026".to_string())]);
        assert!(unmatched.is_empty());
    }

    #[test]
    fn a_multi_item_purchase_is_never_matched() {
        let purchases = vec![purchase(
            vec!["Industrial Decor Pack", "Cargo Heli Small Backpack", "Bar Games Pack"],
            Some(25.17),
            "6 Aug, 2026",
            "row-2",
        )];
        // Even if one of the pack's names happens to also be a candidate
        // name, a multi-item row must never be auto-matched/priced.
        let candidates = vec![(1, "Industrial Decor Pack".to_string(), 252490)];

        let (matched, unmatched) = match_store_purchases(&purchases, &candidates);

        assert!(matched.is_empty());
        assert_eq!(unmatched.len(), 1);
        assert_eq!(unmatched[0].row_id, "row-2");
    }

    #[test]
    fn a_single_item_purchase_with_no_name_match_is_unmatched() {
        let purchases = vec![purchase(vec!["Nonexistent Skin"], Some(5.0), "1 Jan, 2026", "row-3")];
        let candidates = vec![(672, "Bamboo Cage Fridge".to_string(), 252490)];

        let (matched, unmatched) = match_store_purchases(&purchases, &candidates);

        assert!(matched.is_empty());
        assert_eq!(unmatched.len(), 1);
        assert_eq!(unmatched[0].row_id, "row-3");
    }

    #[test]
    fn duplicate_candidate_names_are_each_consumed_only_once() {
        let purchases = vec![
            purchase(vec!["Widget"], Some(1.0), "1 Jan, 2026", "row-a"),
            purchase(vec!["Widget"], Some(2.0), "2 Jan, 2026", "row-b"),
        ];
        // Two vault items share the same name — the oldest-listed candidate
        // (id 10) must be consumed by the first purchase, and the second
        // candidate (id 11) by the second purchase, not the same one twice.
        let candidates = vec![(10, "Widget".to_string(), 252490), (11, "Widget".to_string(), 252490)];

        let (matched, unmatched) = match_store_purchases(&purchases, &candidates);

        assert!(unmatched.is_empty());
        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0], (10, "Widget".to_string(), 1.0, "1 Jan, 2026".to_string()));
        assert_eq!(matched[1], (11, "Widget".to_string(), 2.0, "2 Jan, 2026".to_string()));
    }

    #[test]
    fn a_single_item_purchase_with_no_total_price_is_unmatched() {
        // total_price is None (e.g. an unparsed/blank wht_total) — nothing
        // to attribute as the item's price, so this must not silently apply
        // a bogus price; treat as unmatched like any other unresolvable row.
        let purchases = vec![purchase(vec!["Bamboo Cage Fridge"], None, "18 Jun, 2026", "row-4")];
        let candidates = vec![(672, "Bamboo Cage Fridge".to_string(), 252490)];

        let (matched, unmatched) = match_store_purchases(&purchases, &candidates);

        assert!(matched.is_empty());
        assert_eq!(unmatched.len(), 1);
        assert_eq!(unmatched[0].row_id, "row-4");
    }

    #[test]
    fn a_name_match_with_a_different_appid_is_unmatched() {
        // Same name, but the purchase's appid (a different game) doesn't
        // match the candidate's — must not cross-match between games.
        let mut steam_deck_widget = purchase(vec!["Widget"], Some(5.0), "1 Jan, 2026", "row-5");
        steam_deck_widget.appid = Some(999999);
        let candidates = vec![(672, "Widget".to_string(), 252490)];

        let (matched, unmatched) = match_store_purchases(&[steam_deck_widget], &candidates);

        assert!(matched.is_empty());
        assert_eq!(unmatched.len(), 1);
        assert_eq!(unmatched[0].row_id, "row-5");
    }

    #[test]
    fn a_purchase_with_no_appid_still_matches_by_name_alone() {
        // Some store-history rows (e.g. HelpWithTransaction links) never
        // carry an appid at all, even for a real single-item purchase — see
        // the `appid` field's doc comment on StorePurchase. Absence must
        // fall back to name-only matching, not auto-reject.
        let mut warhammer = purchase(vec!["Rust Warhammer Pack"], Some(10.23), "1 Jan, 2026", "row-6");
        warhammer.appid = None;
        let candidates = vec![(900, "Rust Warhammer Pack".to_string(), 252490)];

        let (matched, unmatched) = match_store_purchases(&[warhammer], &candidates);

        assert_eq!(matched, vec![(900, "Rust Warhammer Pack".to_string(), 10.23, "1 Jan, 2026".to_string())]);
        assert!(unmatched.is_empty());
    }
}
