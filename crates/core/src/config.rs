//! Daemon configuration, read from /etc/ghadd/config.toml (all fields optional).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG_PATH: &str = "/etc/ghadd/config.toml";
pub const DEFAULT_SOCKET_PATH: &str = "/run/ghadd.sock";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub hosts_path: PathBuf,
    pub state_dir: PathBuf,
    pub socket_path: PathBuf,
    /// Holds the argon2 hash of the friend password.
    pub lock_path: PathBuf,
    /// Blocklist URLs, hosts or domain-per-line format.
    pub sources: Vec<String>,
    /// How often the hosts file is checked and repaired.
    pub check_interval_secs: u64,
    /// Set the immutable flag on managed files so they can't be edited casually.
    pub make_immutable: bool,
    /// Filtering DNS servers forced through systemd-resolved and NetworkManager.
    pub dns_servers: Vec<String>,
    pub manage_dns: bool,
    /// Turn off browser DNS-over-HTTPS and force SafeSearch through browser policies.
    pub manage_browsers: bool,
    pub youtube_restricted: bool,
    /// Prefix for managed system files other than the hosts file (tests use a scratch dir).
    pub system_root: PathBuf,
    /// Reload systemd-resolved / NetworkManager after changing their config.
    pub run_hooks: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hosts_path: "/etc/hosts".into(),
            state_dir: "/var/lib/ghadd".into(),
            socket_path: DEFAULT_SOCKET_PATH.into(),
            lock_path: "/etc/ghadd/lock".into(),
            sources: vec![
                "https://raw.githubusercontent.com/StevenBlack/hosts/master/alternates/porn-only/hosts".into(),
                "https://raw.githubusercontent.com/4skinSkywalker/Anti-Porn-HOSTS-File/master/HOSTS.txt".into(),
            ],
            check_interval_secs: 30,
            make_immutable: true,
            // Cloudflare for Families: blocks adult content, forces SafeSearch on Google/Bing/DuckDuckGo.
            dns_servers: ["1.1.1.3", "1.0.0.3", "2606:4700:4700::1113", "2606:4700:4700::1003"]
                .map(String::from)
                .to_vec(),
            manage_dns: true,
            manage_browsers: true,
            youtube_restricted: true,
            system_root: "/".into(),
            run_hooks: true,
        }
    }
}

impl Config {
    /// Loads the config; a missing file means all defaults.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).with_context(|| format!("invalid config {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
        }
    }
}
