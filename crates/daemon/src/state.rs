//! Persistent daemon state in <state_dir>/state.json.

use anyhow::{Context, Result};
use ghadd_core::ipc::TamperEvent;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const MAX_TAMPER_EVENTS: usize = 500;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Domains the user added on top of the downloaded lists.
    pub extra_domains: BTreeSet<String>,
    /// Domains from the downloaded lists that were unblocked (needs the friend password).
    pub allowed_domains: BTreeSet<String>,
    /// Switched-on built-in categories (see `blocklist::CATEGORIES`).
    pub categories: BTreeSet<String>,
    pub paused_until: Option<u64>,
    pub disabled: bool,
    /// Path → fingerprint of what we last wrote there (for the hosts file, just
    /// our section). A mismatch means someone else changed it.
    pub applied: BTreeMap<String, u64>,
    pub last_list_update: Option<u64>,
    pub tamper_events: Vec<TamperEvent>,
}

impl State {
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).with_context(|| format!("corrupt {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&tmp, path).with_context(|| format!("cannot write {}", path.display()))
    }

    pub fn record_tamper(&mut self, ts: u64, detail: String) {
        eprintln!("tamper detected: {detail}");
        self.tamper_events.push(TamperEvent { ts, detail });
        let excess = self.tamper_events.len().saturating_sub(MAX_TAMPER_EVENTS);
        self.tamper_events.drain(..excess);
    }
}
