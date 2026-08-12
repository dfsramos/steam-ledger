//! SQLite connection lifecycle: vault path resolution, schema init, seeding.

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

/// Shared, lockable database connection managed by Tauri's app state.
pub struct DbConnection(pub Mutex<Connection>);

/// Resolves `~/.steamledger/vault.db`, creating the parent directory if
/// missing. One-time migration: if `~/.steamledger` doesn't exist yet but
/// `~/.skinledger` does (this app's pre-rebrand name), the whole directory
/// is renamed in place first — otherwise an existing user's real portfolio
/// would silently vanish on their next launch after upgrading. Best-effort:
/// if the rename fails for any reason, the old directory is left untouched
/// (data isn't lost, just not picked up this run) and a fresh, empty
/// `~/.steamledger` is created as usual.
pub fn vault_path() -> PathBuf {
    let home = dirs::home_dir().expect("home dir resolvable");
    let data_dir = home.join(".steamledger");
    let legacy_data_dir = home.join(".skinledger");
    if !data_dir.exists() && legacy_data_dir.exists() {
        let _ = std::fs::rename(&legacy_data_dir, &data_dir);
    }

    let path = data_dir.join("vault.db");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create vault directory");
    }
    path
}

/// Resolves `~/.steamledger/crash.log`, next to the vault. The release build
/// suppresses its console window on Windows (see `main.rs`), so a startup
/// panic is otherwise invisible to the user — this is the only place they
/// (or a future debugging session) can find out what happened.
pub fn crash_log_path() -> PathBuf {
    vault_path()
        .parent()
        .expect("vault_path always has a parent")
        .join("crash.log")
}

/// Applies the schema (idempotent, `CREATE TABLE IF NOT EXISTS`).
fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(include_str!("schema.sql"))
}

/// Reads the set of existing column names for `table` via `PRAGMA
/// table_info`, for deciding which `ALTER TABLE ADD COLUMN`s are still
/// needed on a pre-existing database.
fn existing_columns(conn: &Connection, table: &str) -> rusqlite::Result<std::collections::HashSet<String>> {
    let mut columns = std::collections::HashSet::new();
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get("name")?;
        columns.insert(name);
    }
    Ok(columns)
}

/// Adds columns to `items`/`settings` that were introduced after their
/// `CREATE TABLE IF NOT EXISTS` was first shipped — that statement is a
/// no-op against a table that already exists on disk, so new columns need
/// an explicit `ALTER TABLE` path to reach vault.db files created before
/// this change.
fn migrate_schema(conn: &Connection) -> rusqlite::Result<()> {
    let items_columns = existing_columns(conn, "items")?;
    if !items_columns.contains("appid") {
        conn.execute_batch(
            "ALTER TABLE items ADD COLUMN appid INTEGER NOT NULL DEFAULT 252490;",
        )?;
    }
    if !items_columns.contains("steam_row_id") {
        conn.execute_batch("ALTER TABLE items ADD COLUMN steam_row_id TEXT;")?;
    }
    if !items_columns.contains("game_name") {
        conn.execute_batch("ALTER TABLE items ADD COLUMN game_name TEXT;")?;
    }
    if !items_columns.contains("market_price_updated_at") {
        conn.execute_batch("ALTER TABLE items ADD COLUMN market_price_updated_at TEXT;")?;
    }
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_items_steam_row_id ON items(steam_row_id) WHERE steam_row_id IS NOT NULL;",
    )?;

    let settings_columns = existing_columns(conn, "settings")?;
    if !settings_columns.contains("steam_currency_code") {
        conn.execute_batch(
            "ALTER TABLE settings ADD COLUMN steam_currency_code INTEGER NOT NULL DEFAULT 2;",
        )?;
    }

    Ok(())
}

/// Opens the real vault database, ensures the schema is present. A brand
/// new vault starts genuinely empty — no demo/placeholder items — populated
/// only by what the user adds, pastes, or imports from Steam themselves.
pub fn connect() -> rusqlite::Result<Connection> {
    let conn = Connection::open(vault_path())?;
    // SQLite disables foreign-key enforcement by default per connection —
    // without this, schema.sql's `ON DELETE CASCADE` on price_history is
    // silently inert and remove_item/wipe_vault leak orphaned rows.
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    init_schema(&conn)?;
    migrate_schema(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    #[test]
    fn deleting_an_item_cascades_to_its_price_history() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        super::init_schema(&conn).expect("init schema");

        conn.execute(
            "INSERT INTO items (name, category, price_paid, date_purchased) VALUES ('Test Item', 'Misc', 5.0, '2026-01-01')",
            [],
        )
        .expect("insert test item");
        let item_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO price_history (item_id, price, recorded_at) VALUES (?1, 5.0, '2026-01-01')",
            rusqlite::params![item_id],
        )
        .expect("insert price_history row");

        let history_before: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM price_history WHERE item_id = ?1",
                rusqlite::params![item_id],
                |row| row.get(0),
            )
            .expect("count price_history before delete");
        assert_eq!(history_before, 1);

        conn.execute("DELETE FROM items WHERE id = ?1", rusqlite::params![item_id])
            .expect("delete item");

        let history_after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM price_history WHERE item_id = ?1",
                rusqlite::params![item_id],
                |row| row.get(0),
            )
            .expect("count price_history after delete");
        assert_eq!(history_after, 0, "ON DELETE CASCADE should remove orphaned price_history rows");
    }

    #[test]
    fn connect_never_populates_an_empty_vault_with_placeholder_items() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        super::init_schema(&conn).expect("init schema");
        super::migrate_schema(&conn).expect("migrate schema");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
            .expect("count items");
        assert_eq!(count, 0, "a fresh vault must start empty, not pre-populated with demo data");
    }

    #[test]
    fn migrate_schema_adds_appid_and_steam_row_id_to_a_pre_existing_items_table() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        // Pre-migration schema: no appid/steam_row_id columns.
        conn.execute_batch(
            "CREATE TABLE items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                category TEXT NOT NULL,
                quantity INTEGER NOT NULL DEFAULT 1,
                price_paid REAL NOT NULL,
                market_price REAL NOT NULL DEFAULT 0,
                date_purchased TEXT NOT NULL,
                notes TEXT,
                hue INTEGER NOT NULL DEFAULT 0,
                sold INTEGER NOT NULL DEFAULT 0,
                sold_price REAL,
                sold_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO items (name, category, price_paid, date_purchased) VALUES ('Old Item', 'Misc', 1.0, '2026-01-01');
            CREATE TABLE settings (id INTEGER PRIMARY KEY CHECK (id = 1));
            INSERT INTO settings (id) VALUES (1);",
        )
        .expect("create pre-migration items table");

        super::migrate_schema(&conn).expect("migrate schema");

        let mut stmt = conn.prepare("PRAGMA table_info(items)").expect("prepare pragma");
        let columns: std::collections::HashSet<String> = stmt
            .query_map([], |row| row.get::<_, String>("name"))
            .expect("query table_info")
            .collect::<rusqlite::Result<_>>()
            .expect("collect column names");
        assert!(columns.contains("appid"));
        assert!(columns.contains("steam_row_id"));
        assert!(columns.contains("game_name"));
        assert!(columns.contains("market_price_updated_at"));

        let appid: i64 = conn
            .query_row("SELECT appid FROM items WHERE name = 'Old Item'", [], |row| row.get(0))
            .expect("read back appid for pre-existing row");
        assert_eq!(appid, 252490, "pre-existing rows should default to Rust's appid");
    }

    #[test]
    fn migrate_schema_adds_steam_currency_code_to_a_pre_existing_settings_table() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                refresh_interval_minutes INTEGER NOT NULL DEFAULT 15,
                backup_path TEXT NOT NULL DEFAULT '',
                auto_backup INTEGER NOT NULL DEFAULT 1
            );
            INSERT INTO settings (id, refresh_interval_minutes, backup_path, auto_backup) VALUES (1, 15, '', 1);
            CREATE TABLE items (id INTEGER PRIMARY KEY);",
        )
        .expect("create pre-migration settings table");

        super::migrate_schema(&conn).expect("migrate schema");

        let currency: i64 = conn
            .query_row("SELECT steam_currency_code FROM settings WHERE id = 1", [], |row| row.get(0))
            .expect("read back steam_currency_code");
        assert_eq!(currency, 2, "pre-existing settings row should default to GBP, matching prior hardcoded behavior");
    }

    #[test]
    fn migrate_schema_is_idempotent_on_a_freshly_initialised_db() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        super::init_schema(&conn).expect("init schema");
        super::migrate_schema(&conn).expect("migrate schema should be a no-op when columns already exist");
        super::migrate_schema(&conn).expect("migrate schema should tolerate being run twice");
    }

    /// Regression test for a real crash: `init_schema` alone (not
    /// `migrate_schema`) used to fail against a pre-existing pre-appid
    /// `items` table, because `schema.sql` unconditionally ran `CREATE
    /// UNIQUE INDEX ... ON items(steam_row_id) ...` right after `CREATE
    /// TABLE IF NOT EXISTS items` — a no-op against a table that already
    /// existed — referencing a column that didn't exist yet. This must run
    /// `init_schema` and `migrate_schema` in the exact order `connect()`
    /// does; the other migration tests above call `migrate_schema` alone
    /// and would not have caught this.
    #[test]
    fn connect_sequence_succeeds_against_a_pre_existing_pre_appid_database() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                category TEXT NOT NULL,
                quantity INTEGER NOT NULL DEFAULT 1,
                price_paid REAL NOT NULL,
                market_price REAL NOT NULL DEFAULT 0,
                date_purchased TEXT NOT NULL,
                notes TEXT,
                hue INTEGER NOT NULL DEFAULT 0,
                sold INTEGER NOT NULL DEFAULT 0,
                sold_price REAL,
                sold_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE price_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                price REAL NOT NULL,
                recorded_at TEXT NOT NULL
            );
            CREATE TABLE settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                refresh_interval_minutes INTEGER NOT NULL DEFAULT 15,
                backup_path TEXT NOT NULL DEFAULT '',
                auto_backup INTEGER NOT NULL DEFAULT 1
            );
            INSERT INTO settings (id, refresh_interval_minutes, backup_path, auto_backup) VALUES (1, 15, '', 1);",
        )
        .expect("create pre-appid database, matching a real vault.db from before appid/steam_row_id existed");

        super::init_schema(&conn).expect("init_schema must not fail against a pre-existing items table");
        super::migrate_schema(&conn).expect("migrate schema");

        let mut stmt = conn.prepare("PRAGMA table_info(items)").expect("prepare pragma");
        let columns: std::collections::HashSet<String> = stmt
            .query_map([], |row| row.get::<_, String>("name"))
            .expect("query table_info")
            .collect::<rusqlite::Result<_>>()
            .expect("collect column names");
        assert!(columns.contains("appid"));
        assert!(columns.contains("steam_row_id"));
    }
}
