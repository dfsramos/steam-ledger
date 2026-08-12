//! Data models shared between SQLite rows and Tauri command payloads.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    pub name: String,
    pub category: String,
    pub quantity: i64,
    pub price_paid: f64,
    pub market_price: f64,
    pub date_purchased: String,
    pub notes: Option<String>,
    pub hue: i64,
    pub sold: bool,
    pub sold_price: Option<f64>,
    pub sold_at: Option<String>,
    pub created_at: String,
    pub appid: i64,
    pub steam_row_id: Option<String>,
    pub game_name: Option<String>,
    /// UTC ISO-8601 (`YYYY-MM-DDTHH:MM:SSZ`), set whenever `market_price` is
    /// written by a refresh — `None` means never refreshed.
    pub market_price_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistoryPoint {
    pub id: i64,
    pub item_id: i64,
    pub price: f64,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub refresh_interval_minutes: i64,
    pub backup_path: String,
    pub auto_backup: bool,
    /// Steam's `ECurrencyCode` (1=USD, 2=GBP, 3=EUR, ...) — see
    /// `src/currency.js` on the frontend for the matching symbol table.
    pub steam_currency_code: i64,
}
