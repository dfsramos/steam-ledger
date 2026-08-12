//! Lightweight progress reporting for the long-running Steam import
//! pipeline (history pagination + per-game inventory fetches can easily
//! take tens of seconds). Threaded through as an optional channel sender so
//! callers that don't care — unit tests, the debug tool — can just pass
//! `None` instead of every function needing a Tauri-specific dependency.

pub type ProgressSender = tokio::sync::mpsc::UnboundedSender<String>;

/// Sends `message` if a sender was provided; a dropped/closed receiver is
/// not an error worth propagating, so send failures are silently ignored.
pub fn report(sender: Option<&ProgressSender>, message: impl Into<String>) {
    if let Some(sender) = sender {
        let _ = sender.send(message.into());
    }
}
