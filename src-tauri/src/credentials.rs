//! OS-native credential storage for Steam session cookies (Windows
//! Credential Manager / macOS Keychain / Linux Secret Service via the
//! `keyring` crate). Cookies are never written to SQLite or a plain file.
//!
//! Two separate cookies are stored under two separate keyring entries:
//! Steam scopes session cookies per-domain (confirmed live — a cookie valid
//! on steamcommunity.com is rejected by store.steampowered.com), so Market
//! history/Inventory (steamcommunity.com) and in-game-store purchase history
//! (store.steampowered.com, see `store_history.rs`) each need their own.

const SERVICE: &str = "steam-ledger";
const MARKET_COOKIE_USER: &str = "steam_session_cookie";
const STORE_COOKIE_USER: &str = "steam_store_session_cookie";
// help.steampowered.com is a THIRD independently-scoped login domain —
// neither the Market nor Store cookie authenticates there (confirmed live,
// 2026-08-12: both redirect to its own /en/login page). Used to resolve
// per-item prices for multi-item ("pack") store purchases via the
// authenticated help-wizard page — see help_wizard.rs and
// .mallet/features/steam-store-purchase-import/state.md.
const HELP_COOKIE_USER: &str = "steam_help_session_cookie";

fn entry(user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, user).map_err(|e| e.to_string())
}

fn save(user: &str, cookie: &str) -> Result<(), String> {
    entry(user)?.set_password(cookie).map_err(|e| e.to_string())
}

/// Returns `None` on any error, including "not found" — absence is a normal,
/// expected state (the user hasn't connected an account yet), never an
/// error condition surfaced to the frontend.
fn get(user: &str) -> Option<String> {
    entry(user).ok()?.get_password().ok()
}

/// Treats "not found" as success — there's nothing to clear.
fn clear(user: &str) -> Result<(), String> {
    match entry(user)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn save_steam_cookie(cookie: &str) -> Result<(), String> {
    save(MARKET_COOKIE_USER, cookie)
}

pub fn get_steam_cookie() -> Option<String> {
    get(MARKET_COOKIE_USER)
}

pub fn clear_steam_cookie() -> Result<(), String> {
    clear(MARKET_COOKIE_USER)
}

pub fn save_steam_store_cookie(cookie: &str) -> Result<(), String> {
    save(STORE_COOKIE_USER, cookie)
}

pub fn get_steam_store_cookie() -> Option<String> {
    get(STORE_COOKIE_USER)
}

pub fn clear_steam_store_cookie() -> Result<(), String> {
    clear(STORE_COOKIE_USER)
}

pub fn save_steam_help_cookie(cookie: &str) -> Result<(), String> {
    save(HELP_COOKIE_USER, cookie)
}

pub fn get_steam_help_cookie() -> Option<String> {
    get(HELP_COOKIE_USER)
}

pub fn clear_steam_help_cookie() -> Result<(), String> {
    clear(HELP_COOKIE_USER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_steam_cookie_returns_none_when_nothing_is_saved() {
        // This service/user pair is never written to by any other test or
        // by the app itself, so it's guaranteed unset.
        let unset = keyring::Entry::new("steam-ledger-test-unset", "nobody").ok();
        assert!(unset.is_some());
        assert_eq!(unset.unwrap().get_password().ok(), None);
    }

    // Live round-trip (save -> get -> clear) requires a running OS credential
    // store. This WSL2 dev sandbox has no Secret Service (no D-Bus session —
    // see .mallet/memory.md), so this is `#[ignore]`d here; it should be run
    // on a real Linux desktop or Windows before shipping.
    #[test]
    #[ignore]
    fn saves_gets_and_clears_a_cookie_round_trip() {
        save_steam_cookie("test-cookie-value").expect("save cookie");
        assert_eq!(get_steam_cookie(), Some("test-cookie-value".to_string()));
        clear_steam_cookie().expect("clear cookie");
        assert_eq!(get_steam_cookie(), None);
    }

    #[test]
    #[ignore]
    fn saves_gets_and_clears_a_store_cookie_round_trip_independently_of_the_market_cookie() {
        save_steam_cookie("market-cookie-value").expect("save market cookie");
        save_steam_store_cookie("store-cookie-value").expect("save store cookie");

        assert_eq!(get_steam_cookie(), Some("market-cookie-value".to_string()));
        assert_eq!(get_steam_store_cookie(), Some("store-cookie-value".to_string()));

        clear_steam_store_cookie().expect("clear store cookie");
        assert_eq!(get_steam_store_cookie(), None, "clearing the store cookie must not affect the market one");
        assert_eq!(get_steam_cookie(), Some("market-cookie-value".to_string()));

        clear_steam_cookie().expect("clear market cookie");
        assert_eq!(get_steam_cookie(), None);
    }
}
