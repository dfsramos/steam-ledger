//! Resolves the account's own SteamID64 from the same session cookie
//! already used for market history — no separate cookie or manual entry
//! needed. Confirmed live: `https://steamcommunity.com/my/` (a
//! logged-in-only redirect) lands on the user's own profile page, which
//! embeds `g_steamID = "<digits>";` directly in its HTML. This only works
//! against `steamcommunity.com`, not `store.steampowered.com` — the two
//! domains don't accept the same session cookie (confirmed live: the store
//! account page returned a "Sign In" page for a cookie that authenticated
//! fine here).

const MY_PROFILE_URL: &str = "https://steamcommunity.com/my/";

fn extract_steamid(html: &str) -> Option<String> {
    let re = regex::Regex::new(r#"g_steamID\s*=\s*"(\d+)""#).expect("valid regex");
    re.captures(html).map(|c| c[1].to_string())
}

/// Fetches the user's own profile page and extracts their SteamID64.
pub async fn fetch_steamid(cookie: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent(crate::ua::PRIMARY)
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let response = client
        .get(MY_PROFILE_URL)
        .header("Cookie", format!("steamLoginSecure={cookie}"))
        .send()
        .await
        .map_err(|e| format!("request to {MY_PROFILE_URL} failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "request to {MY_PROFILE_URL} failed with status {}",
            response.status()
        ));
    }

    let html = response
        .text()
        .await
        .map_err(|e| format!("failed to read profile page body: {e}"))?;

    extract_steamid(&html)
        .ok_or_else(|| "could not find g_steamID on the profile page — cookie may be invalid or expired".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_steamid_from_the_real_confirmed_markup() {
        // Matches the real snippet confirmed live:
        // g_sessionID = "..."; g_steamID = "76561198000000001"; g_strLanguage = ...
        let html = r#"g_sessionID = "dafc67fcea10b2d314472024"; g_steamID = "76561198000000001"; g_strLanguage = "english";"#;
        assert_eq!(extract_steamid(html), Some("76561198000000001".to_string()));
    }

    #[test]
    fn returns_none_when_g_steamid_is_absent() {
        let html = r#"<html><title>Sign In</title></html>"#;
        assert_eq!(extract_steamid(html), None);
    }
}
