//! Embedded Steam login: opens a real login page in a Tauri webview window
//! and reads the `steamLoginSecure` cookie directly from that window's
//! cookie jar (via `WebviewWindow::cookies_for_url`, confirmed available on
//! every desktop backend in the vendored tauri 2.11.5 / wry 0.55.1) instead
//! of asking the user to copy it from their own browser's devtools.
//!
//! Steam scopes session cookies per-domain (see `credentials.rs`), but a
//! single login already authenticates all three domains this app needs —
//! that's why a user manually visiting steamcommunity.com,
//! store.steampowered.com, and help.steampowered.com in their own
//! already-logged-in browser finds a valid cookie waiting on each one
//! without re-entering credentials. This replicates that by navigating the
//! same embedded window through each domain in turn once the initial login
//! is confirmed, polling for that domain's cookie to appear.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::credentials;
use crate::progress::{self, ProgressSender};

const WINDOW_LABEL: &str = "steam-login";
const LOGIN_URL: &str = "https://steamcommunity.com/login/home?goto=my/profile/";
const STORE_URL: &str = "https://store.steampowered.com/";
const HELP_URL: &str = "https://help.steampowered.com/";
const COOKIE_NAME: &str = "steamLoginSecure";

// The user has to actually type credentials (and possibly a Steam Guard
// code) for the first domain — generous on purpose. The other two domains
// only need Steam's own cross-domain session transfer to run, which happens
// automatically within a few seconds of the page loading.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(800);

#[derive(Debug, Clone, serde::Serialize)]
pub struct SteamConnectResult {
    pub market: bool,
    pub store: bool,
    pub help: bool,
    /// True if the user closed the login window before the flow finished.
    /// Whatever cookies were already confirmed at that point are still
    /// saved — a partial connect is strictly better than discarding it.
    pub canceled: bool,
}

fn find_cookie(cookies: &[tauri::webview::Cookie<'static>]) -> Option<String> {
    cookies.iter().find(|c| c.name() == COOKIE_NAME).map(|c| c.value().to_string())
}

/// Polls `cookies_for_url(url)` until `steamLoginSecure` appears, the
/// deadline passes, or `canceled` is set by the window's close event.
async fn wait_for_cookie(
    window: &WebviewWindow,
    url: &str,
    timeout: Duration,
    canceled: &Arc<AtomicBool>,
) -> Option<String> {
    let parsed = tauri::Url::parse(url).ok()?;
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        if canceled.load(Ordering::SeqCst) {
            return None;
        }
        if let Ok(cookies) = window.cookies_for_url(parsed.clone()) {
            if let Some(value) = find_cookie(&cookies) {
                return Some(value);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

pub async fn connect(app: tauri::AppHandle) -> Result<SteamConnectResult, String> {
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let emit_app = app.clone();
    tokio::spawn(async move {
        while let Some(message) = progress_rx.recv().await {
            let _ = emit_app.emit("steam-connect-progress", message);
        }
    });
    let progress: Option<&ProgressSender> = Some(&progress_tx);

    // A prior attempt may have been abandoned without ever confirming a
    // cookie (e.g. the app itself was closed mid-flow) — start clean rather
    // than reusing/erroring on a leftover window with the same label.
    if let Some(existing) = app.get_webview_window(WINDOW_LABEL) {
        let _ = existing.close();
    }

    progress::report(progress, "Opening Steam login...");

    let login_url = tauri::Url::parse(LOGIN_URL).map_err(|e| e.to_string())?;
    let window = WebviewWindowBuilder::new(&app, WINDOW_LABEL, WebviewUrl::External(login_url))
        .title("Connect Steam Account")
        .inner_size(480.0, 760.0)
        .build()
        .map_err(|e| e.to_string())?;

    let canceled = Arc::new(AtomicBool::new(false));
    {
        let canceled = canceled.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                canceled.store(true, Ordering::SeqCst);
            }
        });
    }

    progress::report(progress, "Waiting for you to sign in...");
    let market_cookie = wait_for_cookie(&window, "https://steamcommunity.com", LOGIN_TIMEOUT, &canceled).await;

    let mut store_cookie = None;
    let mut help_cookie = None;

    if market_cookie.is_some() && !canceled.load(Ordering::SeqCst) {
        progress::report(progress, "Signed in. Connecting store.steampowered.com...");
        if window.navigate(tauri::Url::parse(STORE_URL).map_err(|e| e.to_string())?).is_ok() {
            store_cookie = wait_for_cookie(&window, STORE_URL, TRANSFER_TIMEOUT, &canceled).await;
        }
        progress::report(
            progress,
            if store_cookie.is_some() {
                "Connected store.steampowered.com."
            } else {
                "Could not confirm store.steampowered.com — add it manually below if needed."
            },
        );
    }

    if market_cookie.is_some() && !canceled.load(Ordering::SeqCst) {
        progress::report(progress, "Connecting help.steampowered.com...");
        if window.navigate(tauri::Url::parse(HELP_URL).map_err(|e| e.to_string())?).is_ok() {
            help_cookie = wait_for_cookie(&window, HELP_URL, TRANSFER_TIMEOUT, &canceled).await;
        }
        progress::report(
            progress,
            if help_cookie.is_some() {
                "Connected help.steampowered.com."
            } else {
                "Could not confirm help.steampowered.com — this one is optional."
            },
        );
    }

    let was_canceled = canceled.load(Ordering::SeqCst);
    if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
        let _ = w.close();
    }

    if let Some(value) = &market_cookie {
        credentials::save_steam_cookie(value)?;
    }
    if let Some(value) = &store_cookie {
        credentials::save_steam_store_cookie(value)?;
    }
    if let Some(value) = &help_cookie {
        credentials::save_steam_help_cookie(value)?;
    }

    progress::report(
        progress,
        if was_canceled {
            "Window closed before finishing."
        } else if market_cookie.is_some() {
            "Done."
        } else {
            "Sign-in was not detected — no cookies saved."
        },
    );

    Ok(SteamConnectResult {
        market: market_cookie.is_some(),
        store: store_cookie.is_some(),
        help: help_cookie.is_some(),
        canceled: was_canceled,
    })
}
