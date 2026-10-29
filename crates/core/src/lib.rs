//! Shared code for the Ghadd daemon, CLI and desktop app.

pub mod blocklist;
pub mod config;
pub mod ipc;
pub mod lock;

/// Seconds since the Unix epoch.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
