CREATE TABLE IF NOT EXISTS items (
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
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  appid INTEGER NOT NULL DEFAULT 252490,
  steam_row_id TEXT,
  game_name TEXT,
  market_price_updated_at TEXT
);
CREATE TABLE IF NOT EXISTS price_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  price REAL NOT NULL,
  recorded_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  refresh_interval_minutes INTEGER NOT NULL DEFAULT 15,
  backup_path TEXT NOT NULL DEFAULT '',
  auto_backup INTEGER NOT NULL DEFAULT 1,
  -- Steam's ECurrencyCode (1=USD, 2=GBP, 3=EUR, ...) — defaults to GBP,
  -- matching this app's prior hardcoded behavior, so existing users see no
  -- surprise change until they explicitly pick their real currency.
  steam_currency_code INTEGER NOT NULL DEFAULT 2
);
INSERT OR IGNORE INTO settings (id, refresh_interval_minutes, backup_path, auto_backup) VALUES (1, 15, '', 1);
