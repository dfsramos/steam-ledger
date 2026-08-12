//! Typed Tauri commands invoked from the frontend via invoke().

use rusqlite::{params, Row};
use tauri::State;

use crate::appinfo;
use crate::credentials;
use crate::date_infer;
use crate::db;
use crate::db::DbConnection;
use crate::help_wizard;
use crate::inventory;
use crate::models::{Item, PriceHistoryPoint, Settings};
use crate::reconcile;
use crate::steam;
use crate::steam_history::{self, SteamTransaction, TransactionAction};
use crate::steamid;
use crate::store_history;
use crate::store_reconcile;

/// App-wide flag checked cooperatively by `refresh_prices_command` between
/// items; set by `cancel_price_refresh`, reset at the start of every run.
pub struct RefreshCancelFlag(pub std::sync::atomic::AtomicBool);

fn item_from_row(row: &Row) -> rusqlite::Result<Item> {
    Ok(Item {
        id: row.get("id")?,
        name: row.get("name")?,
        category: row.get("category")?,
        quantity: row.get("quantity")?,
        price_paid: row.get("price_paid")?,
        market_price: row.get("market_price")?,
        date_purchased: row.get("date_purchased")?,
        notes: row.get("notes")?,
        hue: row.get("hue")?,
        sold: row.get("sold")?,
        sold_price: row.get("sold_price")?,
        sold_at: row.get("sold_at")?,
        created_at: row.get("created_at")?,
        appid: row.get("appid")?,
        steam_row_id: row.get("steam_row_id")?,
        game_name: row.get("game_name")?,
        market_price_updated_at: row.get("market_price_updated_at")?,
    })
}

fn price_history_point_from_row(row: &Row) -> rusqlite::Result<PriceHistoryPoint> {
    Ok(PriceHistoryPoint {
        id: row.get("id")?,
        item_id: row.get("item_id")?,
        price: row.get("price")?,
        recorded_at: row.get("recorded_at")?,
    })
}

#[tauri::command]
pub fn list_items(state: State<DbConnection>) -> Result<Vec<Item>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT * FROM items WHERE sold = 0 ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], item_from_row)
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<Item>>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_item(state: State<DbConnection>, id: i64) -> Result<Option<Item>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.query_row("SELECT * FROM items WHERE id = ?1", params![id], |row| {
        item_from_row(row)
    })
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.to_string()),
    })
}

#[tauri::command]
pub fn sell_item(state: State<DbConnection>, id: i64, sold_price: f64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE items SET sold = 1, sold_price = ?1, sold_at = date('now') WHERE id = ?2",
        params![sold_price, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn remove_item(state: State<DbConnection>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM items WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_price_history(
    state: State<DbConnection>,
    item_id: i64,
) -> Result<Vec<PriceHistoryPoint>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT * FROM price_history WHERE item_id = ?1 ORDER BY recorded_at")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![item_id], price_history_point_from_row)
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<PriceHistoryPoint>>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(state: State<DbConnection>) -> Result<Settings, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT refresh_interval_minutes, backup_path, auto_backup, steam_currency_code FROM settings WHERE id = 1",
        [],
        |row| {
            Ok(Settings {
                refresh_interval_minutes: row.get(0)?,
                backup_path: row.get(1)?,
                auto_backup: row.get(2)?,
                steam_currency_code: row.get(3)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_settings(state: State<DbConnection>, payload: Settings) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE settings SET refresh_interval_minutes = ?1, backup_path = ?2, auto_backup = ?3, steam_currency_code = ?4 WHERE id = 1",
        params![
            payload.refresh_interval_minutes,
            payload.backup_path,
            payload.auto_backup,
            payload.steam_currency_code,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn persist_price(conn: &rusqlite::Connection, id: i64, price: f64) -> Result<(), String> {
    conn.execute(
        "UPDATE items SET market_price = ?1, market_price_updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ?2",
        params![price, id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO price_history (item_id, price, recorded_at) VALUES (?1, ?2, date('now'))",
        params![id, price],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Looks up and persists one item's current market price — the Portfolio
/// screen's per-row refresh action and the Item Detail screen's "Refresh
/// price" button both use this, so a manually-refreshed price actually
/// survives a reload instead of only ever living in frontend memory.
#[tauri::command]
pub async fn refresh_item_price_command(
    state: State<'_, DbConnection>,
    item_id: i64,
) -> Result<Option<f64>, String> {
    let (name, appid, currency_code): (String, i64, i64) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let (name, appid): (String, i64) = conn
            .query_row("SELECT name, appid FROM items WHERE id = ?1", params![item_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| e.to_string())?;
        let currency_code: i64 = conn
            .query_row("SELECT steam_currency_code FROM settings WHERE id = 1", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        (name, appid, currency_code)
    };

    let price = steam::get_market_price(&name, &appid.to_string(), &currency_code.to_string()).await;
    if let Some(price) = price {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        persist_price(&conn, item_id, price)?;
    }
    Ok(price)
}

/// Sets the cooperative cancellation flag `refresh_prices_command` checks
/// between items — best-effort, since it's only observed between requests,
/// not mid-request.
#[tauri::command]
pub fn cancel_price_refresh(cancel: State<'_, RefreshCancelFlag>) {
    cancel.0.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Emits `"price-refresh-progress"` events (payload: `"<done>/<total>"`)
/// throughout — with the 1.1s inter-item rate-limit sleep, a real-sized
/// portfolio (a live account confirmed 294 active items) takes several
/// minutes end to end, and a silent multi-minute wait reads as hung, not
/// busy (the exact bug this fixes — see `.mallet/lessons.md`). The frontend
/// listens for these while `state.refreshing` is true (see
/// `src/screens/portfolio.js`).
///
/// Only refreshes `item_ids` — the caller passes exactly the
/// currently-filtered/searched rows, not every unsold item, so filtering
/// the portfolio view and hitting "Refresh prices" does what it looks like
/// it does instead of silently refreshing everything regardless. Each price
/// is persisted as soon as it resolves (not batched until the end) so a
/// cancelled run keeps whatever progress it already made.
#[tauri::command]
pub async fn refresh_prices_command(
    app: tauri::AppHandle,
    state: State<'_, DbConnection>,
    cancel: State<'_, RefreshCancelFlag>,
    item_ids: Vec<i64>,
) -> Result<steam::RefreshSummary, String> {
    use tauri::Emitter;
    cancel.0.store(false, std::sync::atomic::Ordering::SeqCst);

    if item_ids.is_empty() {
        return Ok(steam::RefreshSummary { updated: 0, skipped: 0, canceled: false });
    }

    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let emit_app = app.clone();
    tokio::spawn(async move {
        while let Some(message) = progress_rx.recv().await {
            let _ = emit_app.emit("price-refresh-progress", message);
        }
    });
    let progress = Some(&progress_tx);

    let (items, currency_code): (Vec<(i64, String, i64)>, i64) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let placeholders = item_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT id, name, appid FROM items WHERE sold = 0 AND id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(item_ids.iter()), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|e| e.to_string())?;
        let items = rows
            .collect::<rusqlite::Result<Vec<(i64, String, i64)>>>()
            .map_err(|e| e.to_string())?;
        let currency_code: i64 = conn
            .query_row("SELECT steam_currency_code FROM settings WHERE id = 1", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        (items, currency_code)
    };

    let mut updated = 0i64;
    let mut skipped = 0i64;
    let mut canceled = false;
    let len = items.len();
    for (index, (id, name, appid)) in items.into_iter().enumerate() {
        if cancel.0.load(std::sync::atomic::Ordering::SeqCst) {
            canceled = true;
            break;
        }

        match steam::get_market_price(&name, &appid.to_string(), &currency_code.to_string()).await {
            Some(price) => {
                updated += 1;
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                persist_price(&conn, id, price)?;
            }
            None => skipped += 1,
        }

        crate::progress::report(progress, format!("{}/{len}", index + 1));

        if index + 1 < len {
            tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        }
    }

    Ok(steam::RefreshSummary { updated, skipped, canceled })
}

/// CSV header (matches the column order written per row). `notes` is
/// excluded since it may contain commas and this command has no
/// CSV-escaping logic.
const EXPORT_CSV_HEADER: &str =
    "id,name,category,quantity,price_paid,market_price,date_purchased,sold,sold_price,sold_at";

#[tauri::command]
pub fn export_items_csv(state: State<DbConnection>) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT * FROM items ORDER BY id")
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], item_from_row)
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<Item>>>()
        .map_err(|e| e.to_string())?;

    let mut csv = String::from(EXPORT_CSV_HEADER);
    csv.push('\n');
    for item in items {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            item.id,
            item.name,
            item.category,
            item.quantity,
            item.price_paid,
            item.market_price,
            item.date_purchased,
            item.sold as i64,
            item.sold_price.map(|p| p.to_string()).unwrap_or_default(),
            item.sold_at.unwrap_or_default(),
        ));
    }

    Ok(csv)
}

#[tauri::command]
pub fn export_items_json(state: State<DbConnection>) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT * FROM items ORDER BY id")
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], item_from_row)
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<Item>>>()
        .map_err(|e| e.to_string())?;

    serde_json::to_string_pretty(&items).map_err(|e| e.to_string())
}

/// Destructive: deletes every item (cascades to price_history) and resets
/// settings to their schema defaults. The frontend guards this behind a
/// two-step confirm.
#[tauri::command]
pub fn wipe_vault(state: State<DbConnection>) -> Result<(), String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM items", []).map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE settings SET refresh_interval_minutes = 15, backup_path = '', auto_backup = 1 WHERE id = 1",
        [],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_vault_file_size() -> Result<u64, String> {
    std::fs::metadata(db::vault_path())
        .map(|m| m.len())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_steam_cookie_command(cookie: String) -> Result<(), String> {
    crate::credentials::save_steam_cookie(&cookie)
}

#[tauri::command]
pub fn has_steam_cookie_command() -> bool {
    crate::credentials::get_steam_cookie().is_some()
}

#[tauri::command]
pub fn clear_steam_cookie_command() -> Result<(), String> {
    crate::credentials::clear_steam_cookie()
}

#[tauri::command]
pub fn save_steam_store_cookie_command(cookie: String) -> Result<(), String> {
    crate::credentials::save_steam_store_cookie(&cookie)
}

#[tauri::command]
pub fn has_steam_store_cookie_command() -> bool {
    crate::credentials::get_steam_store_cookie().is_some()
}

#[tauri::command]
pub fn clear_steam_store_cookie_command() -> Result<(), String> {
    crate::credentials::clear_steam_store_cookie()
}

#[tauri::command]
pub fn save_steam_help_cookie_command(cookie: String) -> Result<(), String> {
    crate::credentials::save_steam_help_cookie(&cookie)
}

#[tauri::command]
pub fn has_steam_help_cookie_command() -> bool {
    crate::credentials::get_steam_help_cookie().is_some()
}

#[tauri::command]
pub fn clear_steam_help_cookie_command() -> Result<(), String> {
    crate::credentials::clear_steam_help_cookie()
}

/// Opens an embedded Steam login window and reads the resulting session
/// cookie directly from all three domains this app needs, instead of
/// requiring the user to copy each one out of their own browser's devtools
/// — see `steam_login.rs`. Emits `"steam-connect-progress"` events
/// throughout, the same pattern `preview_steam_import` uses.
#[tauri::command]
pub async fn connect_steam_account(app: tauri::AppHandle) -> Result<crate::steam_login::SteamConnectResult, String> {
    crate::steam_login::connect(app).await
}

/// A store-history purchase matched by exact name (and appid, when known)
/// to an existing vault item still at `price_paid = 0.0` — see
/// `store_reconcile::match_store_purchases`. Applying one is an `UPDATE` to
/// an *existing* row, unlike the rest of the Steam import flow, which only
/// ever inserts new items.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PriceFill {
    pub item_id: i64,
    pub item_name: String,
    pub appid: i64,
    pub price: f64,
    pub date: String,
}

/// A multi-item ("pack") store purchase that can never be auto-matched to
/// specific vault items — Steam exposes a pack's name and total price but
/// never which individual skins are inside (approved design, `state.md`,
/// 2026-08-12). Informational only: never applied on commit, never counted
/// toward the commit item count.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FlaggedPack {
    pub item_names: Vec<String>,
    pub appid: Option<i64>,
    pub total_price: Option<f64>,
    pub date: String,
    pub currency: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SteamImportPreview {
    pub transactions: Vec<SteamTransaction>,
    pub price_fills: Vec<PriceFill>,
    pub flagged_packs: Vec<FlaggedPack>,
}

/// Fetches the user's Steam market history (paginating from scratch on
/// first sync, or only newly-added pages on repeat syncs — see
/// `steam_history::sync_history`), resolves each transaction's year-less
/// `raw_date` into a concrete `"YYYY-MM-DD"` via `date_infer::infer_years`,
/// then reconciles against the user's *current* Steam inventory
/// (`reconcile::reconcile_holdings_with_history`) so a purchase later
/// resold via the market doesn't get imported as if still owned. `Sold`
/// transactions are always returned unchanged (informational — see
/// `commit_steam_import`); `Bought` transactions are only returned if a
/// currently-held unit was matched to them (or synthesized, for holdings
/// with no purchase on record — gifted/traded/crafted).
///
/// When a `store.steampowered.com` cookie is also saved
/// (`credentials::get_steam_store_cookie`), additionally fetches the
/// account's in-game *store* purchase history and matches single-item
/// purchases to vault items still at `price_paid = 0.0` (items bought via
/// the store rather than the Community Market never got a real price from
/// the Market-history path above). Entirely skipped, with no error, when no
/// store cookie is saved — this is additive, not required.
///
/// Emits `"steam-import-progress"` events (string payloads) throughout —
/// this can easily take tens of seconds for a large account, and a
/// perfectly silent multi-second wait reads as broken, not busy. The
/// frontend listens for these while `state.steamImportLoading` is true (see
/// `src/screens/add.js`).
#[tauri::command]
pub async fn preview_steam_import(
    app: tauri::AppHandle,
    state: State<'_, DbConnection>,
) -> Result<SteamImportPreview, String> {
    use tauri::Emitter;
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let emit_app = app.clone();
    tokio::spawn(async move {
        while let Some(message) = progress_rx.recv().await {
            let _ = emit_app.emit("steam-import-progress", message);
        }
    });
    let progress = Some(&progress_tx);

    let cookie = credentials::get_steam_cookie()
        .ok_or_else(|| "No Steam session cookie saved".to_string())?;

    crate::progress::report(progress, "Starting Steam import...");

    let (already_imported, current_year, already_imported_counts) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;

        let mut stmt = conn
            .prepare("SELECT steam_row_id FROM items WHERE steam_row_id IS NOT NULL")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let already_imported = rows
            .collect::<rusqlite::Result<std::collections::HashSet<String>>>()
            .map_err(|e| e.to_string())?;

        let current_year_text: String = conn
            .query_row("SELECT strftime('%Y', 'now')", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let current_year: i32 = current_year_text.parse().map_err(|_| {
            format!("could not parse current year from SQLite: {current_year_text}")
        })?;

        // An incremental sync only re-fetches NEW history rows, so
        // reconciliation must know how many units of each item are already
        // reflected in the ledger from a prior import — otherwise it would
        // fabricate a duplicate "unmatched" entry for a holding that's
        // already correctly imported, every single re-sync.
        let mut counts_stmt = conn
            .prepare(
                "SELECT appid, name, COUNT(*) FROM items WHERE steam_row_id IS NOT NULL GROUP BY appid, name",
            )
            .map_err(|e| e.to_string())?;
        let counts_rows = counts_stmt
            .query_map([], |row| {
                Ok(((row.get::<_, i64>(0)?, row.get::<_, String>(1)?), row.get::<_, i64>(2)?))
            })
            .map_err(|e| e.to_string())?;
        let already_imported_counts: std::collections::HashMap<(i64, String), i64> = counts_rows
            .collect::<rusqlite::Result<_>>()
            .map_err(|e| e.to_string())?;

        (already_imported, current_year, already_imported_counts)
    };

    crate::progress::report(progress, "Fetching your Steam market history...");
    let mut transactions = steam_history::sync_history(&cookie, &already_imported, progress).await?;

    let raw_dates: Vec<String> = transactions.iter().map(|t| t.raw_date.clone()).collect();
    let inferred_dates = date_infer::infer_years(&raw_dates, current_year);
    for (transaction, inferred_date) in transactions.iter_mut().zip(inferred_dates) {
        transaction.raw_date = inferred_date;
    }

    // Only games/contexts with activity in this fetch need an inventory
    // check — a game with zero new history rows this sync has no possible
    // reconciliation delta to resolve.
    let appid_contextid_pairs: std::collections::HashSet<(i64, String)> = transactions
        .iter()
        .map(|t| (t.appid, t.contextid.clone()))
        .collect();

    crate::progress::report(progress, "Resolving your Steam account...");
    let steamid = steamid::fetch_steamid(&cookie).await?;

    // One game's inventory failing (confirmed live: a real account hit a
    // 500 for one appid/contextid while six others succeeded fine) must not
    // abort the whole import — that game's transactions fall back to the
    // pre-reconciliation (unfiltered) behavior instead of being silently
    // dropped or blocking every other game.
    let mut holdings: std::collections::HashMap<(i64, String), i64> = std::collections::HashMap::new();
    // Whether at least one currently-held unit of an item is marketable
    // right now — see `inventory::HoldingInfo` for why `reconcile` needs
    // this (distinguishing genuinely non-tradeable items from ones merely
    // stuck in Steam's temporary post-purchase market hold).
    let mut marketable: std::collections::HashMap<(i64, String), bool> = std::collections::HashMap::new();
    let mut failed_pairs: std::collections::HashSet<(i64, String)> = std::collections::HashSet::new();
    for (appid, contextid) in appid_contextid_pairs {
        match inventory::fetch_holdings(&cookie, &steamid, appid, &contextid, progress).await {
            Ok(info_map) => {
                for (name, info) in info_map {
                    let key = (appid, name);
                    *holdings.entry(key.clone()).or_insert(0) += info.count;
                    *marketable.entry(key).or_insert(false) |= info.any_marketable;
                }
            }
            Err(e) => {
                eprintln!("preview_steam_import: inventory fetch failed for appid={appid} contextid={contextid}, falling back to unreconciled history for it: {e}");
                crate::progress::report(
                    progress,
                    format!("Couldn't check inventory for appid {appid} — showing its unreviewed history instead."),
                );
                failed_pairs.insert((appid, contextid));
            }
        }
    }

    for ((appid, name), count) in already_imported_counts {
        if let Some(held) = holdings.get_mut(&(appid, name)) {
            *held = (*held - count).max(0);
        }
    }

    // Resolved via Steam's public appdetails API, used only as a fallback for
    // synthetic (no-history) entries whose game name can't be read off a
    // matching transaction — see `reconcile::reconcile_holdings_with_history`.
    // Best-effort: a failed lookup just leaves that appid out of the map, and
    // reconcile falls back further to the raw appid.
    crate::progress::report(progress, "Resolving app names...");
    let distinct_appids: std::collections::HashSet<i64> = holdings.keys().map(|(appid, _)| *appid).collect();
    let mut app_names: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    for appid in distinct_appids {
        if let Some(name) = appinfo::fetch_app_name(appid).await {
            app_names.insert(appid, name);
        }
    }

    crate::progress::report(progress, "Comparing history with your current inventory...");
    let (unreconciled, reconcilable): (Vec<_>, Vec<_>) = transactions
        .into_iter()
        .partition(|t| failed_pairs.contains(&(t.appid, t.contextid.clone())));

    let mut result =
        reconcile::reconcile_holdings_with_history(&holdings, &marketable, &app_names, reconcilable);
    result.extend(unreconciled);

    let importable = result.iter().filter(|t| matches!(t.action, TransactionAction::Bought)).count();
    crate::progress::report(progress, format!("Done — {importable} item(s) ready to review."));

    let (price_fills, flagged_packs) = match credentials::get_steam_store_cookie() {
        Some(store_cookie) => {
            // Candidates are every vault item still unpriced — a store
            // purchase can only ever fill in a price, never introduce a new
            // item, so once matched and committed an item's `price_paid`
            // stops being 0.0 and it naturally drops out of future syncs;
            // no separate "already imported" row-id tracking is needed the
            // way `steam_row_id` provides for Market history.
            let candidates: Vec<(i64, String, i64)> = {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                let mut stmt = conn
                    .prepare("SELECT id, name, appid FROM items WHERE price_paid = 0.0 ORDER BY id")
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
                    })
                    .map_err(|e| e.to_string())?;
                rows.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())?
            };

            // A store purchase can also match a transaction that's about to
            // be created FRESH by this same preview (a synthetic "held but
            // no matching Market-history purchase" entry — a store-bought
            // item never has Market history at all, so reconcile always
            // leaves it exactly this shape: `Bought` with `price: None`).
            // These have no DB id yet — nothing is committed until the user
            // reviews and confirms — so a match here can't go through the
            // UPDATE-based `PriceFill` path below (there's no row to
            // update); instead it's applied directly to `result` so the
            // review table shows the real price/date immediately, the same
            // way a real Market-history match would. See
            // `apply_store_matches_to_pending_transactions`.
            let has_pending = result
                .iter()
                .any(|t| matches!(t.action, TransactionAction::Bought) && t.price.is_none());

            if candidates.is_empty() && !has_pending {
                (Vec::new(), Vec::new())
            } else {
                crate::progress::report(progress, "Fetching your Steam store purchase history...");
                let appid_by_item: std::collections::HashMap<i64, i64> =
                    candidates.iter().map(|(id, _, appid)| (*id, *appid)).collect();

                let purchases = store_history::sync_store_history(
                    &store_cookie,
                    &std::collections::HashSet::new(),
                    progress,
                )
                .await?;
                let (matched, unmatched_after_db) =
                    store_reconcile::match_store_purchases(&purchases, &candidates);

                let (pending_matched_count, unmatched) =
                    apply_store_matches_to_pending_transactions(&mut result, unmatched_after_db);

                let already_matched_db_ids: std::collections::HashSet<i64> =
                    matched.iter().map(|(id, ..)| *id).collect();
                let mut price_fills: Vec<PriceFill> = matched
                    .into_iter()
                    .map(|(item_id, item_name, price, date)| {
                        // `date` here is Steam's literal "18 Jun, 2026" text
                        // (see StorePurchase::date) — must be reformatted to
                        // the "YYYY-MM-DD" shape items.date_purchased is
                        // stored in everywhere else, or downstream date math
                        // (e.g. detail-derive.js's heldForLabel) silently
                        // breaks on it. `unwrap_or(date)` only matters if
                        // Steam's format ever changes; every real value seen
                        // live parses cleanly.
                        let normalized_date = store_history::parse_store_date(&date).unwrap_or(date);
                        PriceFill {
                            item_id,
                            item_name,
                            appid: appid_by_item.get(&item_id).copied().unwrap_or_default(),
                            price,
                            date: normalized_date,
                        }
                    })
                    .collect();

                // Multi-item ("pack") purchases can't match here — a pack
                // row never has a single item_names entry — but Steam's
                // authenticated help-wizard page (help_wizard.rs) exposes a
                // real per-item price breakdown for exactly these, IF the
                // user has saved a third, separately-scoped cookie for
                // help.steampowered.com. Best-effort and entirely additive:
                // a pack that can't be resolved (no cookie, expired
                // session, fetch failure) falls straight through to
                // `flagged_packs` exactly as before.
                let (pack_price_fills, unmatched) = match credentials::get_steam_help_cookie() {
                    Some(help_cookie) => {
                        resolve_pack_breakdowns(
                            &help_cookie,
                            unmatched,
                            &candidates,
                            &already_matched_db_ids,
                            &mut result,
                            progress,
                        )
                        .await
                    }
                    None => (Vec::new(), unmatched),
                };
                price_fills.extend(pack_price_fills);

                // `unmatched` also holds single-item rows that never matched
                // any candidate (no vault item, no pending entry, or
                // already priced) — those aren't actionable and aren't
                // packs, so only genuine multi-item rows still unresolved
                // after the help-wizard pass are surfaced as flagged.
                let flagged_packs: Vec<FlaggedPack> = unmatched
                    .into_iter()
                    .filter(|p| p.item_names.len() > 1)
                    .map(|p| FlaggedPack {
                        item_names: p.item_names,
                        appid: p.appid,
                        total_price: p.total_price,
                        date: p.date,
                        currency: p.currency,
                    })
                    .collect();

                crate::progress::report(
                    progress,
                    format!(
                        "Found {} store price fill(s) ({} on new items) and {} flagged pack purchase(s).",
                        price_fills.len() + pending_matched_count,
                        pending_matched_count,
                        flagged_packs.len()
                    ),
                );

                (price_fills, flagged_packs)
            }
        }
        None => (Vec::new(), Vec::new()),
    };

    Ok(SteamImportPreview { transactions: result, price_fills, flagged_packs })
}

/// Matches store purchases against transactions in `result` that are about
/// to be created fresh by this same preview with an unknown price (a
/// synthetic "held, no matching Market-history purchase" entry — see the
/// call site in `preview_steam_import`), and applies each match directly to
/// the transaction's `price`/`raw_date` in place. Returns the number
/// applied and whatever store purchases still didn't match anything (for
/// the caller to filter down to flagged packs).
fn apply_store_matches_to_pending_transactions(
    result: &mut [SteamTransaction],
    unmatched_purchases: Vec<store_history::StorePurchase>,
) -> (usize, Vec<store_history::StorePurchase>) {
    let pending_candidates: Vec<(i64, String, i64)> = result
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t.action, TransactionAction::Bought) && t.price.is_none())
        .map(|(i, t)| (i as i64, t.market_hash_name.clone(), t.appid))
        .collect();

    let (matched_pending, unmatched) =
        store_reconcile::match_store_purchases(&unmatched_purchases, &pending_candidates);

    for (index, _name, price, date) in &matched_pending {
        let idx = *index as usize;
        result[idx].price = Some(*price);
        // Unlike `PriceFill::date` (a plain SQL column, tolerant of any
        // string), `raw_date` here is rendered as an HTML `<input
        // type="date">` value and used as-is at commit — an unparsed
        // fallback would produce an invalid date input or a bad
        // `date_purchased`, so leave it as reconcile's own empty-string
        // placeholder (renders "n/a", defaults to today's date at commit)
        // if the format is ever unrecognized.
        if let Some(normalized) = store_history::parse_store_date(date) {
            result[idx].raw_date = normalized;
        }
    }

    (matched_pending.len(), unmatched)
}

/// Resolves multi-item ("pack") store purchases into real per-item price
/// fills via Steam's authenticated help-wizard page (`help_wizard.rs`),
/// which is the only place Steam exposes which price goes to which item
/// within a pack — the wallet-history table itself only ever has the
/// pack's name and total. Best-effort per pack: a fetch failure (expired
/// help cookie, unexpected page shape, missing `appid`) just leaves that
/// pack in the returned list for the caller to flag as before — this is a
/// refinement on top of an already-usable fallback, not something the rest
/// of the import depends on. Single-item purchases pass through untouched
/// (they were never matched upstream for a reason unrelated to being a
/// pack — no vault item, no pending entry, or already priced — and a
/// per-item breakdown wouldn't change that).
///
/// `already_matched_db_ids` seeds which DB candidates the *earlier*
/// single-item pass already claimed, so two different packs (or a pack and
/// a single-item purchase) can never both claim the same still-unpriced
/// vault item; matches against `result`'s pending transactions instead
/// naturally exclude anything already resolved, since a resolved
/// transaction's `price` is no longer `None`.
async fn resolve_pack_breakdowns(
    help_cookie: &str,
    purchases: Vec<store_history::StorePurchase>,
    db_candidates: &[(i64, String, i64)],
    already_matched_db_ids: &std::collections::HashSet<i64>,
    result: &mut [SteamTransaction],
    progress: Option<&crate::progress::ProgressSender>,
) -> (Vec<PriceFill>, Vec<store_history::StorePurchase>) {
    let mut price_fills = Vec::new();
    let mut still_unmatched = Vec::new();
    let mut consumed_db_ids = already_matched_db_ids.clone();

    for purchase in purchases {
        let Some(appid) = purchase.appid.filter(|_| purchase.item_names.len() > 1) else {
            // Not a pack, or a pack with no known appid (the help-wizard
            // URL requires one) — nothing this function can do.
            still_unmatched.push(purchase);
            continue;
        };

        let breakdown = help_wizard::fetch_pack_breakdown(help_cookie, &purchase.row_id, appid).await;
        // Same rate-limit courtesy as every other sequential Steam fetch
        // loop in this app.
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

        let Some(items) = breakdown else {
            still_unmatched.push(purchase);
            continue;
        };

        let normalized_date =
            store_history::parse_store_date(&purchase.date).unwrap_or_else(|| purchase.date.clone());

        let mut any_matched = false;
        for item in &items {
            if let Some(&(item_id, _, _)) = db_candidates
                .iter()
                .find(|(id, name, cand_appid)| *name == item.name && *cand_appid == appid && !consumed_db_ids.contains(id))
            {
                consumed_db_ids.insert(item_id);
                price_fills.push(PriceFill {
                    item_id,
                    item_name: item.name.clone(),
                    appid,
                    price: item.price,
                    date: normalized_date.clone(),
                });
                any_matched = true;
                continue;
            }

            if let Some(idx) = result.iter().position(|t| {
                matches!(t.action, TransactionAction::Bought)
                    && t.price.is_none()
                    && t.market_hash_name == item.name
                    && t.appid == appid
            }) {
                result[idx].price = Some(item.price);
                result[idx].raw_date = normalized_date.clone();
                any_matched = true;
            }
        }

        // A pack that resolved at least one item is dropped from
        // `flagged_packs` entirely — there's no "partially resolved, flag
        // the rest" shape today, and applying what could be matched is
        // strictly better than re-flagging a pack that's already been
        // meaningfully priced.
        if !any_matched {
            still_unmatched.push(purchase);
        }
    }

    if !price_fills.is_empty() {
        crate::progress::report(
            progress,
            format!("Resolved {} flagged pack item(s) via Steam's purchase-detail page.", price_fills.len()),
        );
    }

    (price_fills, still_unmatched)
}

/// Persists confirmed `Bought` transactions as new items (plus a matching
/// same-day `price_history` row), skipping any whose `row_id` is already
/// present as a defensive re-check against double-import. `Sold`
/// transactions are informational only — this app doesn't model market
/// sales as reducing an existing holding, so they're never inserted here.
///
/// `price_fills` (store-purchase matches, see `preview_steam_import`) are
/// applied as `UPDATE`s to existing rows rather than inserts — the `AND
/// price_paid = 0.0` guard is the same defensive re-check pattern as the
/// `row_id` check above, so re-committing a stale preview can't clobber a
/// price the user has since edited by hand.
#[tauri::command]
pub fn commit_steam_import(
    state: State<DbConnection>,
    transactions: Vec<SteamTransaction>,
    price_fills: Vec<PriceFill>,
) -> Result<i64, String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let mut inserted = 0i64;

    // Fallback for synthetic (unmatched-holding) entries, whose `raw_date`
    // is empty — see `reconcile::synthetic_entry`.
    let today: String = tx
        .query_row("SELECT date('now')", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    for transaction in transactions
        .iter()
        .filter(|t| matches!(t.action, TransactionAction::Bought))
    {
        let already_present: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM items WHERE steam_row_id = ?1)",
                params![transaction.row_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if already_present {
            continue;
        }

        // `price: None` and an empty `raw_date` both mean "no purchase on
        // record" (gifted/traded/crafted) — the review table lets the user
        // fill in the real values before committing, but nothing blocks
        // commit if they don't.
        let price = transaction.price.unwrap_or(0.0);
        let date_purchased = if transaction.raw_date.is_empty() {
            today.clone()
        } else {
            transaction.raw_date.clone()
        };

        tx.execute(
            "INSERT INTO items (name, category, quantity, price_paid, market_price, date_purchased, appid, steam_row_id, game_name)
             VALUES (?1, 'Uncategorised', 1, ?2, ?2, ?3, ?4, ?5, ?6)",
            params![
                transaction.market_hash_name,
                price,
                date_purchased,
                transaction.appid,
                transaction.row_id,
                transaction.game_name,
            ],
        )
        .map_err(|e| e.to_string())?;

        let item_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO price_history (item_id, price, recorded_at) VALUES (?1, ?2, ?3)",
            params![item_id, price, date_purchased],
        )
        .map_err(|e| e.to_string())?;

        inserted += 1;
    }

    apply_price_fills(&tx, &price_fills)?;

    tx.commit().map_err(|e| e.to_string())?;
    Ok(inserted)
}

/// Applies each store-purchase price fill as an `UPDATE` to its matched,
/// still-unpriced vault item. The `AND price_paid = 0.0` guard makes this
/// safe to re-run against a stale preview: an item already filled (by this
/// path or hand-edited since) is left untouched rather than clobbered.
fn apply_price_fills(tx: &rusqlite::Transaction, price_fills: &[PriceFill]) -> Result<(), String> {
    for fill in price_fills {
        tx.execute(
            "UPDATE items SET price_paid = ?1, date_purchased = ?2 WHERE id = ?3 AND price_paid = 0.0",
            params![fill.price, fill.date, fill.item_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Backfills `game_name` for rows persisted before that column existed (`NULL`
/// today, e.g. items imported by an older build) via Steam's public
/// appdetails API — see `appinfo.rs`. Best-effort per-appid: a failed lookup
/// just leaves those rows `NULL` for the next attempt rather than failing the
/// whole backfill. Returns the number of rows actually updated so the
/// frontend only needs to reload `state.items` when something changed.
#[tauri::command]
pub async fn backfill_game_names(state: State<'_, DbConnection>) -> Result<i64, String> {
    let distinct_appids: Vec<i64> = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT DISTINCT appid FROM items WHERE game_name IS NULL")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0)).map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<_>>().map_err(|e| e.to_string())?
    };

    if distinct_appids.is_empty() {
        return Ok(0);
    }

    let mut resolved: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    for appid in distinct_appids {
        if let Some(name) = appinfo::fetch_app_name(appid).await {
            resolved.insert(appid, name);
        }
    }

    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut updated = 0i64;
    for (appid, name) in &resolved {
        updated += conn
            .execute(
                "UPDATE items SET game_name = ?1 WHERE appid = ?2 AND game_name IS NULL",
                params![name, appid],
            )
            .map_err(|e| e.to_string())? as i64;
    }
    Ok(updated)
}

#[cfg(test)]
mod apply_price_fills_tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(include_str!("schema.sql")).expect("init schema");
        conn
    }

    fn insert_item(conn: &Connection, name: &str, price_paid: f64, appid: i64) -> i64 {
        conn.execute(
            "INSERT INTO items (name, category, quantity, price_paid, market_price, date_purchased, appid)
             VALUES (?1, 'Uncategorised', 1, ?2, 0, '2026-01-01', ?3)",
            params![name, price_paid, appid],
        )
        .expect("insert test item");
        conn.last_insert_rowid()
    }

    #[test]
    fn fills_price_and_date_on_a_matched_unpriced_item() {
        let mut conn = setup_conn();
        let item_id = insert_item(&conn, "Bamboo Cage Fridge", 0.0, 252490);

        let fills = vec![PriceFill {
            item_id,
            item_name: "Bamboo Cage Fridge".to_string(),
            appid: 252490,
            price: 2.65,
            date: "2026-06-18".to_string(),
        }];

        let tx = conn.transaction().expect("open transaction");
        apply_price_fills(&tx, &fills).expect("apply price fills");
        tx.commit().expect("commit");

        let (price_paid, date_purchased): (f64, String) = conn
            .query_row(
                "SELECT price_paid, date_purchased FROM items WHERE id = ?1",
                params![item_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read back item");
        assert_eq!(price_paid, 2.65);
        assert_eq!(date_purchased, "2026-06-18");
    }

    #[test]
    fn never_overwrites_an_item_that_already_has_a_price() {
        // Guards against re-committing a stale preview clobbering a price
        // the user has since edited by hand, or a fill already applied by
        // a prior commit.
        let mut conn = setup_conn();
        let item_id = insert_item(&conn, "Already Priced", 9.99, 252490);

        let fills = vec![PriceFill {
            item_id,
            item_name: "Already Priced".to_string(),
            appid: 252490,
            price: 1.0,
            date: "2026-01-02".to_string(),
        }];

        let tx = conn.transaction().expect("open transaction");
        apply_price_fills(&tx, &fills).expect("apply price fills");
        tx.commit().expect("commit");

        let price_paid: f64 = conn
            .query_row("SELECT price_paid FROM items WHERE id = ?1", params![item_id], |row| row.get(0))
            .expect("read back item");
        assert_eq!(price_paid, 9.99, "an already-priced item must never be overwritten by a price fill");
    }

    #[test]
    fn only_touches_the_targeted_item_id() {
        let mut conn = setup_conn();
        let target_id = insert_item(&conn, "Target", 0.0, 252490);
        let other_id = insert_item(&conn, "Target", 0.0, 252490);

        let fills = vec![PriceFill {
            item_id: target_id,
            item_name: "Target".to_string(),
            appid: 252490,
            price: 3.5,
            date: "2026-02-01".to_string(),
        }];

        let tx = conn.transaction().expect("open transaction");
        apply_price_fills(&tx, &fills).expect("apply price fills");
        tx.commit().expect("commit");

        let other_price: f64 = conn
            .query_row("SELECT price_paid FROM items WHERE id = ?1", params![other_id], |row| row.get(0))
            .expect("read back other item");
        assert_eq!(other_price, 0.0, "a same-named item with a different id must not be touched");
    }
}

#[cfg(test)]
mod apply_store_matches_to_pending_transactions_tests {
    use super::*;

    fn pending_transaction(name: &str, appid: i64) -> SteamTransaction {
        SteamTransaction {
            row_id: format!("unmatched:{appid}:{name}:0"),
            appid,
            contextid: "2".to_string(),
            market_hash_name: name.to_string(),
            game_name: "Rust".to_string(),
            price: None,
            action: TransactionAction::Bought,
            raw_date: String::new(),
        }
    }

    fn store_purchase(name: &str, appid: Option<i64>, price: f64, date: &str) -> store_history::StorePurchase {
        store_history::StorePurchase {
            row_id: "row-1".to_string(),
            appid,
            date: date.to_string(),
            game_name: "Rust".to_string(),
            item_names: vec![name.to_string()],
            total_price: Some(price),
            currency: "€".to_string(),
        }
    }

    // Regression test for a real bug: a fresh full re-import into a wiped
    // vault never got store price fills, because the original design only
    // ever checked for candidates already sitting in the DB — but a
    // brand-new item doesn't exist in the DB yet at preview time. Reported
    // live: "Bamboo Cage Fridge" showed £0.00 paid despite the real Steam
    // store history clearly showing a €2.65 purchase.
    #[test]
    fn fills_price_and_date_on_a_pending_transaction_with_no_market_history_match() {
        let mut result = vec![pending_transaction("Bamboo Cage Fridge", 252490)];
        let purchases = vec![store_purchase("Bamboo Cage Fridge", Some(252490), 2.65, "18 Jun, 2026")];

        let (applied, unmatched) = apply_store_matches_to_pending_transactions(&mut result, purchases);

        assert_eq!(applied, 1);
        assert!(unmatched.is_empty());
        assert_eq!(result[0].price, Some(2.65));
        assert_eq!(result[0].raw_date, "2026-06-18", "must be reformatted to YYYY-MM-DD, not left as Steam's raw text");
    }

    #[test]
    fn does_not_touch_a_transaction_that_already_has_a_real_market_history_price() {
        let mut real_purchase = pending_transaction("Already Priced", 252490);
        real_purchase.price = Some(9.99);
        real_purchase.raw_date = "2026-01-01".to_string();
        let mut result = vec![real_purchase];
        let purchases = vec![store_purchase("Already Priced", Some(252490), 1.0, "1 Jan, 2026")];

        let (applied, unmatched) = apply_store_matches_to_pending_transactions(&mut result, purchases);

        assert_eq!(applied, 0, "a transaction with a real Market-history price must not be overwritten by a store match");
        assert_eq!(unmatched.len(), 1);
        assert_eq!(result[0].price, Some(9.99));
    }

    #[test]
    fn does_not_touch_a_sold_transaction_even_with_a_matching_name() {
        let mut sold = pending_transaction("Sold Item", 252490);
        sold.action = TransactionAction::Sold;
        sold.price = Some(5.0);
        let mut result = vec![sold];
        let purchases = vec![store_purchase("Sold Item", Some(252490), 2.0, "1 Jan, 2026")];

        let (applied, unmatched) = apply_store_matches_to_pending_transactions(&mut result, purchases);

        assert_eq!(applied, 0);
        assert_eq!(unmatched.len(), 1);
    }

    #[test]
    fn leaves_raw_date_untouched_when_the_store_date_is_unparseable() {
        let mut result = vec![pending_transaction("Weird Date Item", 252490)];
        let purchases = vec![store_purchase("Weird Date Item", Some(252490), 3.0, "not a real date")];

        let (applied, _unmatched) = apply_store_matches_to_pending_transactions(&mut result, purchases);

        assert_eq!(applied, 1, "price still applies even if the date can't be parsed");
        assert_eq!(result[0].price, Some(3.0));
        assert_eq!(result[0].raw_date, "", "must not write an unparsed string into a field rendered as <input type=date>");
    }

    #[test]
    fn a_multi_item_pack_purchase_never_matches_a_pending_transaction() {
        let mut result = vec![pending_transaction("Industrial Decor Pack", 252490)];
        let purchase = store_history::StorePurchase {
            row_id: "row-pack".to_string(),
            appid: Some(252490),
            date: "6 Aug, 2026".to_string(),
            game_name: "Rust".to_string(),
            item_names: vec!["Industrial Decor Pack".to_string(), "Bar Games Pack".to_string()],
            total_price: Some(25.17),
            currency: "€".to_string(),
        };

        let (applied, unmatched) = apply_store_matches_to_pending_transactions(&mut result, vec![purchase]);

        assert_eq!(applied, 0);
        assert_eq!(unmatched.len(), 1);
        assert_eq!(result[0].price, None);
    }
}
