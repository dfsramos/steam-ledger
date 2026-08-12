//! Throwaway diagnostic tool: calls the exact same fetch/parse path as the
//! app's "Steam import" tab directly, printing to a real console instead of
//! being silently swallowed by the app's `.catch(console.error)` (which is
//! invisible in a packaged release build with no devtools). Not part of the
//! shipped app — run manually when the Steam import flow needs debugging.

#[tokio::main]
async fn main() {
    if std::env::args().any(|a| a == "--test-keyring") {
        test_keyring_roundtrip();
        return;
    }

    if std::env::args().any(|a| a == "--find-steamid") {
        find_steamid().await;
        return;
    }

    if std::env::args().any(|a| a == "--full-preview") {
        full_preview_debug().await;
        return;
    }

    if std::env::args().any(|a| a == "--incremental-sync-check") {
        incremental_sync_check().await;
        return;
    }

    if std::env::args().any(|a| a == "--test-price") {
        test_single_price().await;
        return;
    }

    if std::env::args().any(|a| a == "--save-store-cookie") {
        save_store_cookie();
        return;
    }

    if std::env::args().any(|a| a == "--dump-store-history") {
        dump_store_history().await;
        return;
    }

    if std::env::args().any(|a| a == "--test-store-pagination") {
        test_store_pagination().await;
        return;
    }

    if std::env::args().any(|a| a == "--test-store-reconcile") {
        test_store_reconcile().await;
        return;
    }

    if std::env::args().any(|a| a == "--dump-market-history") {
        dump_market_history().await;
        return;
    }

    if std::env::args().any(|a| a == "--dump-help-wizard") {
        dump_help_wizard().await;
        return;
    }

    if std::env::args().any(|a| a == "--save-help-cookie") {
        save_help_cookie();
        return;
    }

    if std::env::args().any(|a| a == "--test-pack-breakdown") {
        test_pack_breakdown().await;
        return;
    }

    let cookie = match steam_ledger_lib::credentials::get_steam_cookie() {
        Some(c) => c,
        None => {
            println!("No Steam cookie saved in the OS keyring — nothing to test.");
            std::process::exit(1);
        }
    };
    println!("Cookie found (length={}, not printing the value).", cookie.len());

    let already_imported = std::collections::HashSet::new();
    println!("Calling sync_history — this walks the full history on a first sync, watch for per-page progress below...");

    match steam_ledger_lib::steam_history::sync_history(&cookie, &already_imported, None).await {
        Ok(transactions) => {
            println!("SUCCESS: fetched {} transaction(s).", transactions.len());
            for t in transactions.iter().take(5) {
                println!("  {t:?}");
            }
            if transactions.len() > 5 {
                println!("  ... and {} more", transactions.len() - 5);
            }
        }
        Err(e) => {
            println!("ERROR from sync_history: {e}");
        }
    }
}

/// Mirrors `commands::preview_steam_import` exactly (steamid -> history ->
/// date inference -> inventory per (appid,contextid) -> reconcile), but
/// standalone (no DB, so `already_imported`/`already_imported_counts` are
/// empty — this exercises a "fresh preview" scenario, not incremental
/// sync). Prints counts and a sample of the reconciled result so the fix
/// can be confirmed against a real account before trusting the app itself.
async fn full_preview_debug() {
    let cookie = match steam_ledger_lib::credentials::get_steam_cookie() {
        Some(c) => c,
        None => {
            println!("No Steam cookie saved in the OS keyring — nothing to test.");
            std::process::exit(1);
        }
    };

    println!("Fetching steamid...");
    let steamid = match steam_ledger_lib::steamid::fetch_steamid(&cookie).await {
        Ok(id) => {
            println!("steamid: {id}");
            id
        }
        Err(e) => {
            println!("ERROR fetching steamid: {e}");
            return;
        }
    };

    println!("Fetching history (full — no already_imported filter in this standalone run)...");
    let already_imported = std::collections::HashSet::new();
    let mut transactions =
        match steam_ledger_lib::steam_history::sync_history(&cookie, &already_imported, None).await {
            Ok(t) => t,
            Err(e) => {
                println!("ERROR sync_history: {e}");
                return;
            }
        };
    println!("Fetched {} raw transactions", transactions.len());

    let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
    let current_year_text: String = conn
        .query_row("SELECT strftime('%Y','now')", [], |row| row.get(0))
        .expect("get current year");
    let current_year: i32 = current_year_text.parse().expect("parse year");

    let raw_dates: Vec<String> = transactions.iter().map(|t| t.raw_date.clone()).collect();
    let inferred = steam_ledger_lib::date_infer::infer_years(&raw_dates, current_year);
    for (t, d) in transactions.iter_mut().zip(inferred) {
        t.raw_date = d;
    }

    let pairs: std::collections::HashSet<(i64, String)> =
        transactions.iter().map(|t| (t.appid, t.contextid.clone())).collect();
    println!("Distinct (appid, contextid) pairs seen in history: {pairs:?}");

    let mut holdings: std::collections::HashMap<(i64, String), i64> = std::collections::HashMap::new();
    let mut marketable: std::collections::HashMap<(i64, String), bool> = std::collections::HashMap::new();
    for (appid, contextid) in pairs {
        match steam_ledger_lib::inventory::fetch_holdings(&cookie, &steamid, appid, &contextid, None).await {
            Ok(info_map) => {
                let total: i64 = info_map.values().map(|h| h.count).sum();
                let distinct_names = info_map.len();
                let marketable_names = info_map.values().filter(|h| h.any_marketable).count();
                println!(
                    "  inventory appid={appid} contextid={contextid}: {distinct_names} distinct names ({marketable_names} currently marketable), {total} total units held"
                );
                for (name, info) in info_map {
                    let key = (appid, name);
                    *holdings.entry(key.clone()).or_insert(0) += info.count;
                    *marketable.entry(key).or_insert(false) |= info.any_marketable;
                }
            }
            Err(e) => println!("  ERROR fetching inventory appid={appid} contextid={contextid}: {e}"),
        }
    }
    println!("Total distinct held (appid, name) keys across all pairs: {}", holdings.len());

    println!("Resolving app names via appdetails...");
    let distinct_appids: std::collections::HashSet<i64> = holdings.keys().map(|(appid, _)| *appid).collect();
    let mut app_names: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    for appid in distinct_appids {
        match steam_ledger_lib::appinfo::fetch_app_name(appid).await {
            Some(name) => {
                println!("  appid {appid} -> {name}");
                app_names.insert(appid, name);
            }
            None => println!("  appid {appid} -> (unresolved)"),
        }
    }

    let reconciled = steam_ledger_lib::reconcile::reconcile_holdings_with_history(
        &holdings,
        &marketable,
        &app_names,
        transactions,
    );
    use steam_ledger_lib::steam_history::TransactionAction;
    let bought_count = reconciled.iter().filter(|t| matches!(t.action, TransactionAction::Bought)).count();
    let sold_count = reconciled.iter().filter(|t| matches!(t.action, TransactionAction::Sold)).count();
    let unknown_price_count = reconciled.iter().filter(|t| t.price.is_none()).count();
    let total = reconciled.len();
    println!(
        "RECONCILED: {total} total | {bought_count} importable (Bought) | {sold_count} informational (Sold) | {unknown_price_count} with unknown price"
    );

    println!("Sample of importable rows:");
    for t in reconciled.iter().filter(|t| matches!(t.action, TransactionAction::Bought)).take(15) {
        println!(
            "  appid={} game={:?} {:?} price={:?} date={}",
            t.appid, t.game_name, t.market_hash_name, t.price, t.raw_date
        );
    }
}

/// Calls `steam_history::sync_history` exactly the way
/// `commands::preview_steam_import` does — real vault, real
/// already-imported `steam_row_id` set — to verify the fix for the
/// "fetch for all games found nothing to commit" bug: a partial
/// (game-filtered) prior commit used to make the incremental walk stop at
/// the first already-known row and silently drop everything past it.
async fn incremental_sync_check() {
    let cookie = match steam_ledger_lib::credentials::get_steam_cookie() {
        Some(c) => c,
        None => {
            println!("No Steam cookie saved in the OS keyring — nothing to test.");
            std::process::exit(1);
        }
    };

    let conn = steam_ledger_lib::db::connect().expect("open real vault db");
    println!("Opened real vault at {}", steam_ledger_lib::db::vault_path().display());

    let already_imported: std::collections::HashSet<String> = {
        let mut stmt = conn
            .prepare("SELECT steam_row_id FROM items WHERE steam_row_id IS NOT NULL")
            .expect("prepare already_imported query");
        let rows = stmt.query_map([], |row| row.get::<_, String>(0)).expect("query already_imported");
        rows.collect::<rusqlite::Result<_>>().expect("collect already_imported")
    };
    println!("Vault currently has {} already-imported steam_row_id(s)", already_imported.len());

    println!("Running the real incremental sync_history walk...");
    match steam_ledger_lib::steam_history::sync_history(&cookie, &already_imported, None).await {
        Ok(transactions) => {
            use steam_ledger_lib::steam_history::TransactionAction;
            let bought = transactions.iter().filter(|t| matches!(t.action, TransactionAction::Bought)).count();
            let by_appid: std::collections::HashMap<i64, usize> =
                transactions.iter().fold(std::collections::HashMap::new(), |mut acc, t| {
                    *acc.entry(t.appid).or_insert(0) += 1;
                    acc
                });
            println!(
                "Incremental sync returned {} new transaction(s) ({} Bought), by appid: {:?}",
                transactions.len(),
                bought,
                by_appid
            );
        }
        Err(e) => println!("ERROR sync_history: {e}"),
    }
}

/// Fetches https://store.steampowered.com/account/ with the saved cookie
/// and searches for a SteamID64 (17 digits, always prefixed "7656119" for
/// individual accounts) so the real markup can be confirmed before writing
/// a parser against it. Prints only the matched id(s) plus ~60 chars of
/// surrounding context per match — never the full page, since the account
/// page can contain other account details (email, real name) that don't
/// need to pass through here. A steamid64 itself isn't sensitive (it's the
/// basis of the public profile URL), so it's fine to print in full.
async fn find_steamid() {
    let cookie = match steam_ledger_lib::credentials::get_steam_cookie() {
        Some(c) => c,
        None => {
            println!("No Steam cookie saved in the OS keyring — nothing to test.");
            std::process::exit(1);
        }
    };

    let client = reqwest::Client::builder()
        .user_agent(steam_ledger_lib::ua::PRIMARY)
        .build()
        .expect("build reqwest client");

    let url = std::env::args()
        .position(|a| a == "--find-steamid")
        .and_then(|i| std::env::args().nth(i + 1))
        .unwrap_or_else(|| "https://store.steampowered.com/account/".to_string());
    println!("Fetching: {url}");

    let response = match client
        .get(&url)
        .header("Cookie", format!("steamLoginSecure={cookie}"))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            println!("Request failed: {e}");
            return;
        }
    };

    println!("Final URL after redirects: {}", response.url());
    println!("HTTP status: {}", response.status());
    let html = response.text().await.expect("read response body");
    println!("Page length: {} bytes", html.len());

    if let Some(pos) = std::env::args().position(|a| a == "--save-body") {
        if let Some(path) = std::env::args().nth(pos + 1) {
            std::fs::write(&path, &html).expect("write response body to file");
            println!("Saved raw body to {path}");
        }
    }

    let steamid_re = regex::Regex::new(r"7656119\d{10}").expect("valid regex");
    let mut seen = std::collections::HashSet::new();
    let mut match_count = 0;
    for m in steamid_re.find_iter(&html) {
        match_count += 1;
        if !seen.insert(m.as_str().to_string()) {
            continue;
        }
        let start = m.start().saturating_sub(60);
        let end = (m.end() + 20).min(html.len());
        // Byte offsets from the regex may not land on a UTF-8 char
        // boundary; from_utf8_lossy tolerates that instead of panicking.
        let context = String::from_utf8_lossy(&html.as_bytes()[start..end]).replace('\n', " ");
        println!("Match: {}", m.as_str());
        println!("  context: ...{context}...");
    }
    println!(
        "Total occurrences: {match_count}, distinct values: {}",
        seen.len()
    );

    if std::env::args().any(|a| a == "--dump-snippet") {
        let snippet_len = html.len().min(600);
        println!("First {snippet_len} bytes of body:\n{}", &html[..snippet_len]);

        for marker in ["\"descriptions\"", "\"market_hash_name\"", "\"total_inventory_count\""] {
            if let Some(pos) = html.find(marker) {
                let start = pos.saturating_sub(20);
                let end = (pos + 200).min(html.len());
                let ctx = String::from_utf8_lossy(&html.as_bytes()[start..end]);
                println!("--- around {marker} ---\n{ctx}");
            } else {
                println!("marker {marker} NOT FOUND");
            }
        }
    }

    // No 7656119-prefixed match found — print cheap, low-sensitivity signals
    // to tell a login/redirect page apart from a real account page, without
    // dumping the full body.
    if match_count == 0 {
        let title_re = regex::Regex::new(r"(?is)<title>(.*?)</title>").expect("valid regex");
        if let Some(cap) = title_re.captures(&html) {
            println!("Page <title>: {}", cap[1].trim());
        }
        for marker in [
            "g_steamID",
            "account_pulldown",
            "login",
            "Sign In",
            "profiles/",
            "steamid",
        ] {
            let count = html.matches(marker).count();
            println!("occurrences of {marker:?}: {count}");
        }
    }
}

/// Fetches the raw `myhistory/render` JSON directly (bypassing
/// `steam_history::sync_history`'s parsing) to diagnose why a real sync
/// reported `total_count=0` for an account with substantial known history —
/// distinguishes "Steam genuinely reports zero" from "the response isn't
/// what sync_history assumes it is" (auth redirect, different shape, etc.).
async fn dump_market_history() {
    let cookie = match steam_ledger_lib::credentials::get_steam_cookie() {
        Some(c) => c,
        None => {
            println!("No Steam cookie saved in the OS keyring — nothing to test.");
            std::process::exit(1);
        }
    };
    println!("Cookie found (length={}, not printing the value).", cookie.len());

    let client = reqwest::Client::builder()
        .user_agent(steam_ledger_lib::ua::PRIMARY)
        .build()
        .expect("build reqwest client");

    let url = "https://steamcommunity.com/market/myhistory/render/?query=&start=0&count=10";
    let response = match client
        .get(url)
        .header("Cookie", format!("steamLoginSecure={cookie}"))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            println!("Request failed: {e}");
            return;
        }
    };

    println!("Final URL after redirects: {}", response.url());
    println!("HTTP status: {}", response.status());
    let body = response.text().await.expect("read response body");
    println!("Body length: {} bytes", body.len());
    println!("First 500 bytes of body:\n{}", &body[..body.len().min(500)]);
}

/// Fetches a purchase's help-center wizard page
/// (`help.steampowered.com/en/wizard/HelpWithItemPurchase?transid=...`) —
/// the same URL a `StorePurchase` row's `onclick` link points to (its
/// `transid` IS `StorePurchase::row_id`, see store_history.rs's
/// `transid_re`). Investigating whether this exposes a per-item price
/// breakdown for multi-item ("pack") purchases, which `store_history.rs`
/// currently can't get from the wallet-history table itself (only the
/// pack's name and total price) — see the flagged-packs design note in
/// `.mallet/features/steam-store-purchase-import/state.md`.
///
/// `help.steampowered.com` is a third Steam subdomain, distinct from both
/// `steamcommunity.com` (Market) and `store.steampowered.com` (this
/// feature's store cookie) — cookie scoping here hasn't been confirmed
/// live yet, so this tries the store cookie first (same `steampowered.com`
/// parent domain) and reports plainly if that doesn't work, rather than
/// guessing.
///
/// Usage: `--dump-help-wizard <transid> <appid>`
async fn dump_help_wizard() {
    let args: Vec<String> = std::env::args().collect();
    let flag_pos = args.iter().position(|a| a == "--dump-help-wizard").expect("flag present");
    let transid = args.get(flag_pos + 1).expect("usage: --dump-help-wizard <transid> <appid>");
    let appid = args.get(flag_pos + 2).expect("usage: --dump-help-wizard <transid> <appid>");

    let store_cookie = steam_ledger_lib::credentials::get_steam_store_cookie();
    let market_cookie = steam_ledger_lib::credentials::get_steam_cookie();
    let help_cookie = steam_ledger_lib::credentials::get_steam_help_cookie();

    let url = format!("https://help.steampowered.com/en/wizard/HelpWithItemPurchase?transid={transid}&appid={appid}");

    for (label, cookie) in [("store cookie", store_cookie), ("market cookie", market_cookie), ("help cookie", help_cookie)] {
        let Some(cookie) = cookie else {
            println!("--- trying with {label}: none saved, skipping ---");
            continue;
        };
        println!("--- trying with {label} ---");

        let client = reqwest::Client::builder()
            .user_agent(steam_ledger_lib::ua::PRIMARY)
            .build()
            .expect("build reqwest client");

        let response = match client
            .get(&url)
            .header("Cookie", format!("steamLoginSecure={cookie}"))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                println!("Request failed: {e}");
                continue;
            }
        };

        println!("Final URL after redirects: {}", response.url());
        println!("HTTP status: {}", response.status());
        let body = response.text().await.expect("read response body");
        println!("Body length: {} bytes", body.len());

        let path = format!("help-wizard-{label}.html", label = label.replace(' ', "-"));
        std::fs::write(&path, &body).expect("write response body to file");
        println!("Saved raw body to {path}");

        let title_re = regex::Regex::new(r"(?is)<title>(.*?)</title>").expect("valid regex");
        if let Some(cap) = title_re.captures(&body) {
            println!("Page <title>: {}", cap[1].trim());
        }
        for marker in ["Sign In", "login", "market_hash_name", "itemid", "price", "each item"] {
            let count = body.matches(marker).count();
            println!("occurrences of {marker:?}: {count}");
        }
    }
}

/// Independently exercises the same keyring save/get/clear path the app's
/// Settings screen uses, with a throwaway value — isolates whether Windows
/// Credential Manager itself is the problem, separate from whether the UI
/// actually called save.
fn test_keyring_roundtrip() {
    const TEST_VALUE: &str = "steam-ledger-diagnostic-roundtrip-value";

    print!("save... ");
    match steam_ledger_lib::credentials::save_steam_cookie(TEST_VALUE) {
        Ok(()) => println!("ok"),
        Err(e) => {
            println!("FAILED: {e}");
            return;
        }
    }

    print!("get... ");
    match steam_ledger_lib::credentials::get_steam_cookie() {
        Some(v) if v == TEST_VALUE => println!("ok (matches)"),
        Some(v) => println!("MISMATCH: got a different value back (len={})", v.len()),
        None => println!("FAILED: got None right after a successful save"),
    }

    print!("clear... ");
    match steam_ledger_lib::credentials::clear_steam_cookie() {
        Ok(()) => println!("ok"),
        Err(e) => println!("FAILED: {e}"),
    }

    print!("get after clear... ");
    match steam_ledger_lib::credentials::get_steam_cookie() {
        None => println!("ok (None, as expected)"),
        Some(_) => println!("UNEXPECTED: still returns a value after clear"),
    }
}

/// Exercises `steam::get_market_price` directly against the real endpoint,
/// no cookie or DB needed (priceoverview is public/unauthenticated) — used
/// to confirm the fix for the `Accept-Encoding: gzip` 429 bug against a
/// real, previously-affected item ("Cargo Heli Hatchet", appid 252490).
async fn test_single_price() {
    let price = steam_ledger_lib::steam::get_market_price("Cargo Heli Hatchet", "252490", "2").await;
    match price {
        Some(p) => println!("SUCCESS: resolved price = {p}"),
        None => println!("FAILED: got None"),
    }
}

/// Exercises the exact same store-purchase price-fill path as
/// `commands::preview_steam_import`/`commit_steam_import`
/// (steam-store-purchase-import task 04), but directly against the REAL
/// vault.db rather than through the Tauri app — this project's established
/// pattern for live-verifying backend logic without an input-simulation
/// tool available (see `.mallet/lessons.md`). This is task 05's live
/// verification, so it deliberately applies the matched price fills for
/// real (not a dry run) — printing what it's about to do first.
async fn test_store_reconcile() {
    let cookie = match steam_ledger_lib::credentials::get_steam_store_cookie() {
        Some(c) => c,
        None => {
            println!("No store cookie saved — run --save-store-cookie first.");
            std::process::exit(1);
        }
    };

    let mut conn = steam_ledger_lib::db::connect().expect("open real vault db");
    println!("Opened real vault at {}", steam_ledger_lib::db::vault_path().display());

    let candidates: Vec<(i64, String, i64)> = {
        let mut stmt = conn
            .prepare("SELECT id, name, appid FROM items WHERE price_paid = 0.0 ORDER BY id")
            .expect("prepare candidates query");
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
            })
            .expect("query candidates");
        rows.collect::<rusqlite::Result<_>>().expect("collect candidates")
    };
    println!("Candidates (unpriced items): {}", candidates.len());

    println!("Fetching store purchase history...");
    let purchases = steam_ledger_lib::store_history::sync_store_history(
        &cookie,
        &std::collections::HashSet::new(),
        None,
    )
    .await
    .expect("sync_store_history");
    println!("Fetched {} store purchase(s)", purchases.len());

    let (matched, unmatched) =
        steam_ledger_lib::store_reconcile::match_store_purchases(&purchases, &candidates);
    println!("Matched (price fills): {}", matched.len());
    for (item_id, name, price, date) in &matched {
        println!("  id={item_id} name={name:?} price={price} date={date}");
    }

    let packs: Vec<_> = unmatched.iter().filter(|p| p.item_names.len() > 1).collect();
    println!("Flagged pack purchases: {}", packs.len());
    for p in &packs {
        println!("  {:?} total={:?} date={}", p.item_names, p.total_price, p.date);
    }
    println!(
        "Other unmatched single-item rows (no candidate / no price): {}",
        unmatched.len() - packs.len()
    );

    if matched.is_empty() {
        println!("Nothing to apply.");
        return;
    }

    println!("Applying {} price fill(s) to the real vault...", matched.len());
    let tx = conn.transaction().expect("open transaction");
    for (item_id, _name, price, date) in &matched {
        // `date` is Steam's literal "18 Jun, 2026" text — must be
        // reformatted to "YYYY-MM-DD" like commands.rs's production path
        // does, or this writes the same real bug straight into the vault
        // (confirmed: this exact debug flag did that once already before
        // this fix — see .mallet/lessons.md, 2026-08-12).
        let normalized_date = steam_ledger_lib::store_history::parse_store_date(date)
            .unwrap_or_else(|| date.clone());
        let updated = tx
            .execute(
                "UPDATE items SET price_paid = ?1, date_purchased = ?2 WHERE id = ?3 AND price_paid = 0.0",
                rusqlite::params![price, normalized_date, item_id],
            )
            .expect("apply price fill");
        if updated == 0 {
            println!("  WARNING: id={item_id} was NOT updated (already priced by the time this ran?)");
        }
    }
    tx.commit().expect("commit transaction");
    println!("Done.");
}

/// Saves a store.steampowered.com-scoped `steamLoginSecure` value into the
/// OS keyring, read from stdin rather than a CLI arg (a CLI arg would land
/// in shell history) — run this yourself via `! <command>`, piping the
/// value in, so the raw cookie never has to be pasted into chat. Separate
/// keyring entry from the existing Market cookie — see credentials.rs.
fn save_store_cookie() {
    let mut cookie = String::new();
    std::io::stdin().read_line(&mut cookie).expect("read cookie from stdin");
    let cookie = cookie.trim();
    if cookie.is_empty() {
        println!("No cookie value read from stdin — nothing saved.");
        std::process::exit(1);
    }
    match steam_ledger_lib::credentials::save_steam_store_cookie(cookie) {
        Ok(()) => println!("Saved store cookie (length={}) to the OS keyring.", cookie.len()),
        Err(e) => println!("FAILED to save store cookie: {e}"),
    }
}

/// Calls `help_wizard::fetch_pack_breakdown` directly (fetch + parse, the
/// exact same function `commands::resolve_pack_breakdowns` calls) — proves
/// the whole pipeline end-to-end against the real page, not just the
/// parser against a saved-then-reparsed fixture.
///
/// Usage: `--test-pack-breakdown <transid> <appid>`
async fn test_pack_breakdown() {
    let args: Vec<String> = std::env::args().collect();
    let flag_pos = args.iter().position(|a| a == "--test-pack-breakdown").expect("flag present");
    let transid = args.get(flag_pos + 1).expect("usage: --test-pack-breakdown <transid> <appid>");
    let appid: i64 = args
        .get(flag_pos + 2)
        .expect("usage: --test-pack-breakdown <transid> <appid>")
        .parse()
        .expect("appid must be a number");

    let cookie = match steam_ledger_lib::credentials::get_steam_help_cookie() {
        Some(c) => c,
        None => {
            println!("No help cookie saved — run --save-help-cookie first.");
            std::process::exit(1);
        }
    };

    match steam_ledger_lib::help_wizard::fetch_pack_breakdown(&cookie, transid, appid).await {
        Some(items) => {
            println!("SUCCESS: {} item(s) resolved:", items.len());
            for item in &items {
                println!("  {:?} -> {}", item.name, item.price);
            }
            let sum: f64 = items.iter().map(|i| i.price).sum();
            println!("Sum: {sum:.2}");
        }
        None => println!("FAILED: got None (cookie expired, unexpected page shape, or fetch error)"),
    }
}

/// Saves a help.steampowered.com-scoped `steamLoginSecure` value —
/// investigation-only, see credentials.rs's `HELP_COOKIE_USER` doc comment.
fn save_help_cookie() {
    let mut cookie = String::new();
    std::io::stdin().read_line(&mut cookie).expect("read cookie from stdin");
    let cookie = cookie.trim();
    if cookie.is_empty() {
        println!("No cookie value read from stdin — nothing saved.");
        std::process::exit(1);
    }
    match steam_ledger_lib::credentials::save_steam_help_cookie(cookie) {
        Ok(()) => println!("Saved help cookie (length={}) to the OS keyring.", cookie.len()),
        Err(e) => println!("FAILED to save help cookie: {e}"),
    }
}

/// Fetches store.steampowered.com/account/history/ with the store cookie
/// and saves the raw HTML for inspection — pagination mechanism (a "Load
/// More Transactions" button, confirmed via community reverse-engineering
/// projects, not a documented HTTP param) needs to be found by inspecting
/// the real embedded JS before a scraper can be built against it.
async fn dump_store_history() {
    let cookie = match steam_ledger_lib::credentials::get_steam_store_cookie() {
        Some(c) => c,
        None => {
            println!("No store cookie saved — run --save-store-cookie first.");
            std::process::exit(1);
        }
    };

    let client = reqwest::Client::builder()
        .user_agent(steam_ledger_lib::ua::PRIMARY)
        .build()
        .expect("build reqwest client");

    let url = "https://store.steampowered.com/account/history/";
    let response = match client
        .get(url)
        .header("Cookie", format!("steamLoginSecure={cookie}"))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            println!("Request failed: {e}");
            return;
        }
    };

    println!("Final URL after redirects: {}", response.url());
    println!("HTTP status: {}", response.status());
    let html = response.text().await.expect("read response body");
    println!("Page length: {} bytes", html.len());

    let path = std::env::args()
        .position(|a| a == "--dump-store-history")
        .and_then(|i| std::env::args().nth(i + 1))
        .unwrap_or_else(|| "/tmp/store-history.html".to_string());
    std::fs::write(&path, &html).expect("write response body to file");
    println!("Saved raw body to {path}");

    println!("wallet_history_table present: {}", html.contains("wallet_history_table"));
    println!("load_more_button present: {}", html.contains("load_more_button"));
    for marker in ["ajaxGetMoreHistory", "GetMoreHistory", "cursor", "LoadMoreTransactions"] {
        if let Some(pos) = html.find(marker) {
            let start = pos.saturating_sub(80);
            let end = (pos + 300).min(html.len());
            let ctx = String::from_utf8_lossy(&html.as_bytes()[start..end]).replace('\n', " ");
            println!("Marker '{marker}' found, context: ...{ctx}...");
        }
    }
}

/// Resolves the open blocker in
/// `.mallet/features/steam-store-purchase-import/state.md`: makes one real
/// `POST https://store.steampowered.com/account/AjaxLoadMoreHistory/` call
/// (form-encoded `cursor`/`sessionid`, values read from the initial page's
/// `g_historyCursor`/`g_sessionID` JS globals) and prints the raw JSON
/// response body verbatim — deliberately NOT deserialized into any struct
/// yet, since the exact field shape beyond `html`/`cursor` (inferred only
/// from the page's own JS call site, never observed live) is exactly what
/// this call is meant to confirm before `store_history.rs`'s parser is
/// written against it.
async fn test_store_pagination() {
    let cookie = match steam_ledger_lib::credentials::get_steam_store_cookie() {
        Some(c) => c,
        None => {
            println!("No store cookie saved — run --save-store-cookie first.");
            std::process::exit(1);
        }
    };

    let client = reqwest::Client::builder()
        .user_agent(steam_ledger_lib::ua::PRIMARY)
        .build()
        .expect("build reqwest client");

    println!("Fetching initial history page to read g_historyCursor / g_sessionID...");
    let response = match client
        .get("https://store.steampowered.com/account/history/")
        .header("Cookie", format!("steamLoginSecure={cookie}"))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            println!("Initial page request failed: {e}");
            return;
        }
    };
    println!("Final URL after redirects: {}", response.url());
    println!("HTTP status: {}", response.status());
    let html = response.text().await.expect("read response body");
    println!("Page length: {} bytes", html.len());

    let cursor_re =
        regex::Regex::new(r"g_historyCursor\s*=\s*(\{.*?\});").expect("valid regex");
    let Some(cursor_json) = cursor_re.captures(&html).map(|c| c[1].to_string()) else {
        println!("Could not find g_historyCursor in the initial page — cannot proceed.");
        return;
    };
    println!("g_historyCursor: {cursor_json}");
    let cursor: serde_json::Value = serde_json::from_str(&cursor_json).expect("cursor is valid JSON");

    let session_re =
        regex::Regex::new(r#"g_sessionID\s*=\s*"([^"]+)""#).expect("valid regex");
    let Some(session_id) = session_re.captures(&html).map(|c| c[1].to_string()) else {
        println!("Could not find g_sessionID in the initial page — cannot proceed.");
        return;
    };
    println!("g_sessionID: {session_id}");

    // Same helper `store_history.rs`'s own fetch_more uses — jQuery's
    // bracket-notation form encoding for a nested `data` object, not one
    // JSON-stringified field (see cursor_form_fields's doc comment; a
    // single-JSON-string encoding here previously looked exactly like "no
    // more pages" for an account that provably has much more history).
    let mut form = steam_ledger_lib::store_history::cursor_form_fields(&cursor);
    form.push(("sessionid".to_string(), session_id.clone()));

    println!("POSTing to AjaxLoadMoreHistory with cursor + sessionid...");
    let response = match client
        .post("https://store.steampowered.com/account/AjaxLoadMoreHistory/")
        .header(
            "Cookie",
            format!("steamLoginSecure={cookie}; sessionid={session_id}"),
        )
        .form(&form)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            println!("AjaxLoadMoreHistory request failed: {e}");
            return;
        }
    };
    println!("HTTP status: {}", response.status());
    let headers = response.headers().clone();
    println!("Content-Type: {:?}", headers.get("content-type"));

    let body = response.text().await.expect("read AjaxLoadMoreHistory response body");
    println!("Response body length: {} bytes", body.len());
    println!("--- RAW RESPONSE BODY ---\n{body}\n--- END RAW RESPONSE BODY ---");

    let path = std::env::args()
        .position(|a| a == "--test-store-pagination")
        .and_then(|i| std::env::args().nth(i + 1))
        .unwrap_or_else(|| "/tmp/store-pagination-response.json".to_string());
    std::fs::write(&path, &body).expect("write AjaxLoadMoreHistory response body to file");
    println!("Saved raw response body to {path}");
}
