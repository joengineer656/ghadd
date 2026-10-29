//! Line-delimited JSON protocol spoken over the daemon's Unix socket.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Status,
    TamperEvents,
    /// Blocking more never needs the friend password.
    AddDomain { domain: String },
    RemoveDomain { domain: String, password: String },
    /// Switch on a built-in category list (no password needed).
    EnableCategory { name: String },
    DisableCategory { name: String, password: String },
    Pause { minutes: u64, password: String },
    Resume,
    Disable { password: String },
    Enable,
    /// `old` is required once a password has been set.
    SetPassword { old: Option<String>, new: String },
    UpdateLists,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Ok { message: String },
    Status(StatusInfo),
    TamperEvents { events: Vec<TamperEvent> },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfo {
    pub active: bool,
    pub disabled: bool,
    pub paused_until: Option<u64>,
    pub password_set: bool,
    pub blocked_count: usize,
    pub extra_domains: Vec<String>,
    pub allowed_domains: Vec<String>,
    /// Names of the switched-on built-in categories.
    #[serde(default)]
    pub categories: Vec<String>,
    pub last_list_update: Option<u64>,
    pub tamper_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TamperEvent {
    pub ts: u64,
    pub detail: String,
}

/// Sends one request to the daemon and waits for its reply.
pub fn send(socket: &Path, req: &Request) -> Result<Response> {
    let mut stream = UnixStream::connect(socket)
        .with_context(|| format!("cannot reach ghaddd at {} (is the service running?)", socket.display()))?;
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    serde_json::from_str(&reply).context("bad reply from ghaddd")
}
