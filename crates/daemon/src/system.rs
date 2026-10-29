//! System files ghaddd manages besides the hosts file: the filtering DNS and
//! browser policies that stop browsers from skipping it.

use ghadd_core::config::Config;
use serde_json::json;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

/// Google's documented SafeSearch and YouTube Restricted Mode (moderate) addresses.
const GOOGLE_SAFESEARCH_IP: &str = "216.239.38.120";
const YOUTUBE_RESTRICT_IP: &str = "216.239.38.119";
const GOOGLE_HOSTS: &[&str] = &["google.com", "www.google.com"];
const YOUTUBE_HOSTS: &[&str] = &[
    "www.youtube.com",
    "m.youtube.com",
    "youtubei.googleapis.com",
    "youtube.googleapis.com",
    "www.youtube-nocookie.com",
];

/// Public DNS-over-HTTPS endpoints a browser could use to skip the filtering DNS.
/// `use-application-dns.net` is Firefox's canary: blocking it turns off automatic DoH.
pub const DOH_HOSTS: &[&str] = &[
    "use-application-dns.net",
    "dns.google",
    "dns.google.com",
    "cloudflare-dns.com",
    "mozilla.cloudflare-dns.com",
    "chrome.cloudflare-dns.com",
    "one.one.one.one",
    "dns.quad9.net",
    "dns9.quad9.net",
    "doh.opendns.com",
    "dns.nextdns.io",
    "dns.adguard-dns.com",
    "doh.mullvad.net",
    "dns.mullvad.net",
];

const HEADER: &str = "Managed by ghaddd (Ghadd). Do not edit: changes are reverted.";

/// Commands to run after a managed file changes, in this order. NetworkManager
/// goes after resolved so that, once blocking is off, it re-pushes each network's DNS.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Hook {
    Resolved,
    NetworkManager,
    FlushDns,
}

pub struct ManagedFile {
    pub path: PathBuf,
    /// `None` means the file should not exist (blocking is off or the feature is disabled).
    pub content: Option<String>,
    pub hook: Option<Hook>,
}

/// Hosts that the hosts section points at SafeSearch addresses.
pub fn pinned_hosts(cfg: &Config) -> Vec<(&'static str, &'static str)> {
    let mut pinned: Vec<_> = GOOGLE_HOSTS.iter().map(|h| (GOOGLE_SAFESEARCH_IP, *h)).collect();
    if cfg.youtube_restricted {
        pinned.extend(YOUTUBE_HOSTS.iter().map(|h| (YOUTUBE_RESTRICT_IP, *h)));
    }
    pinned
}

pub fn managed_files(cfg: &Config, active: bool) -> Vec<ManagedFile> {
    let path = |rel: &str| cfg.system_root.join(rel);
    let dns = active && cfg.manage_dns;
    let browsers = active && cfg.manage_browsers;

    let resolved = format!(
        "# {HEADER}\n[Resolve]\nDNS={}\nFallbackDNS=\nDomains=~.\n",
        cfg.dns_servers.join(" ")
    );
    // Stop NetworkManager handing each network's DNS (e.g. the Wi-Fi router's) to
    // systemd-resolved; its global-dns setting does not override per-link DNS there.
    let network_manager = format!("# {HEADER}\n[main]\ndns=none\nrc-manager=unmanaged\n");
    let resolv_conf = format!("# {HEADER}\nnameserver 127.0.0.53\noptions edns0 trust-ad\n");
    let firefox = json!({ "policies": { "DNSOverHTTPS": { "Enabled": false, "Locked": true } } });
    let mut chromium = json!({
        "DnsOverHttpsMode": "off",
        "ForceGoogleSafeSearch": true,
        "ForceBingSafeSearch": 2,
        // Brave's Tor windows resolve names over Tor, skipping every DNS filter.
        "TorDisabled": true,
    });
    if cfg.youtube_restricted {
        chromium["ForceYouTubeRestrict"] = json!(1);
    }
    let firefox = serde_json::to_string_pretty(&firefox).unwrap() + "\n";
    let chromium = serde_json::to_string_pretty(&chromium).unwrap() + "\n";

    let mut files = vec![
        ManagedFile {
            path: path("etc/systemd/resolved.conf.d/ghadd.conf"),
            content: dns.then_some(resolved),
            hook: Some(Hook::Resolved),
        },
        ManagedFile {
            path: path("etc/NetworkManager/conf.d/ghadd.conf"),
            content: dns.then_some(network_manager),
            hook: Some(Hook::NetworkManager),
        },
        ManagedFile {
            path: path("etc/resolv.conf"),
            content: dns.then_some(resolv_conf),
            hook: None,
        },
        ManagedFile {
            path: path("etc/firefox/policies/policies.json"),
            content: browsers.then_some(firefox),
            hook: None,
        },
    ];
    for dir in ["etc/chromium/policies/managed", "etc/opt/chrome/policies/managed", "etc/brave/policies/managed"] {
        files.push(ManagedFile {
            path: path(dir).join("ghadd.json"),
            content: browsers.then(|| chromium.clone()),
            hook: None,
        });
    }
    files
}

/// Drops DNS servers that a network interface got on its own (e.g. the router's,
/// from DHCP), which resolved would otherwise query alongside the filtering DNS.
/// resolved keeps these across restarts, so this checks every time. Returns the
/// interfaces that were reset.
pub fn revert_link_dns() -> Vec<String> {
    let Ok(out) = Command::new("resolvectl").arg("dns").output() else { return Vec::new() };
    let leaking = links_with_own_dns(&String::from_utf8_lossy(&out.stdout));
    for link in &leaking {
        if let Err(e) = Command::new("resolvectl").args(["revert", link]).status() {
            eprintln!("warning: resolvectl revert {link} failed: {e}");
        }
    }
    leaking
}

/// Parses `resolvectl dns` lines like `Link 2 (wlan0): 192.168.100.1`.
fn links_with_own_dns(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("Link ")?;
            let (name, servers) = rest.split_once("): ")?;
            let name = name.split_once('(')?.1;
            (!servers.trim().is_empty()).then(|| name.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::links_with_own_dns;

    #[test]
    fn finds_links_with_their_own_dns() {
        let out = "Global: 1.1.1.3 1.0.0.3\nLink 1 (lo):\nLink 2 (wlan0): 192.168.100.1\nLink 3 (enp3s0): fe80::1 10.0.0.1\n";
        assert_eq!(links_with_own_dns(out), ["wlan0", "enp3s0"]);
    }
}

pub fn run_hooks(hooks: &BTreeSet<Hook>) {
    let mut hooks = hooks.clone();
    if hooks.contains(&Hook::NetworkManager) {
        hooks.insert(Hook::Resolved);
    }
    for hook in &hooks {
        let cmd: &[&str] = match hook {
            Hook::Resolved => &["systemctl", "try-restart", "systemd-resolved.service"],
            Hook::NetworkManager => &["nmcli", "general", "reload"],
            Hook::FlushDns => &["resolvectl", "flush-caches"],
        };
        match Command::new(cmd[0]).args(&cmd[1..]).status() {
            Ok(s) if s.success() => {}
            other => eprintln!("warning: `{}` failed: {other:?}", cmd.join(" ")),
        }
    }
}
