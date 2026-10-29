//! Parsing blocklists and managing Ghadd's section inside /etc/hosts.

use std::collections::BTreeSet;

pub const BEGIN_MARKER: &str = "# >>> GHADD BEGIN (managed by ghaddd, do not edit) >>>";
pub const END_MARKER: &str = "# <<< GHADD END <<<";

/// Addresses that hosts-format blocklists point blocked domains at.
const SINK_ADDRS: &[&str] = &["0.0.0.0", "127.0.0.1", "::", "::0", "::1"];
const IGNORED_HOSTS: &[&str] = &[
    "localhost",
    "localhost.localdomain",
    "local",
    "broadcasthost",
    "ip6-localhost",
    "ip6-loopback",
];

/// An optional built-in list the user can switch on (`ghadd category on <name>`).
pub struct Category {
    pub name: &'static str,
    pub title: &'static str,
    pub list: &'static str,
}

pub const CATEGORIES: &[Category] = &[Category {
    name: "manga-anime",
    title: "Manga, webtoons, comics & anime",
    list: include_str!("../../../data/blocklists/manga-anime.txt"),
}];

pub fn category(name: &str) -> Option<&'static Category> {
    CATEGORIES.iter().find(|c| c.name == name)
}

/// The domains of a category, each with its `www.` twin.
pub fn category_domains(c: &Category) -> BTreeSet<String> {
    parse_list(c.list)
        .into_iter()
        .flat_map(|d| {
            let bare = d.strip_prefix("www.").unwrap_or(&d).to_string();
            [format!("www.{bare}"), bare]
        })
        .collect()
}

/// Lowercases and validates a domain name; returns `None` for junk, IPs and localhost names.
pub fn normalize_domain(raw: &str) -> Option<String> {
    let d = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    let well_formed = d.len() <= 253
        && d.contains('.')
        && !d.starts_with(['-', '.'])
        && !d.split('.').any(str::is_empty)
        && d.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'));
    let is_ip = d.split('.').all(|label| label.chars().all(|c| c.is_ascii_digit()));
    (well_formed && !is_ip && !IGNORED_HOSTS.contains(&d.as_str())).then_some(d)
}

/// Parses a blocklist in hosts format (`0.0.0.0 example.com`) or plain
/// domain-per-line format. Lines mapping domains to real IPs are ignored.
pub fn parse_list(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut tokens = line.split_whitespace();
        let Some(first) = tokens.next() else { continue };
        if SINK_ADDRS.contains(&first) {
            out.extend(tokens.filter_map(normalize_domain));
        } else if tokens.next().is_none() {
            out.extend(normalize_domain(first));
        }
    }
    out
}

/// Renders the marked hosts section that blocks `domains` and points each
/// `(ip, host)` in `pinned` at a fixed address (used for SafeSearch).
pub fn render_section(domains: &BTreeSet<String>, pinned: &[(&str, &str)]) -> String {
    let mut s = String::with_capacity(domains.len() * 32 + 128);
    s.push_str(BEGIN_MARKER);
    s.push('\n');
    for (ip, host) in pinned {
        s.push_str(&format!("{ip} {host}\n"));
    }
    for d in domains {
        s.push_str("0.0.0.0 ");
        s.push_str(d);
        s.push('\n');
    }
    s.push_str(END_MARKER);
    s.push('\n');
    s
}

/// Byte range of the first Ghadd section, markers and trailing newline included.
/// A section whose end marker was cut off runs to the end of the file.
fn section_range(hosts: &str) -> Option<(usize, usize)> {
    let start = hosts.find(BEGIN_MARKER)?;
    let end = match hosts[start..].find(END_MARKER) {
        Some(rel) => {
            let end = start + rel + END_MARKER.len();
            if hosts[end..].starts_with('\n') { end + 1 } else { end }
        }
        None => hosts.len(),
    };
    Some((start, end))
}

pub fn extract_section(hosts: &str) -> Option<&str> {
    section_range(hosts).map(|(s, e)| &hosts[s..e])
}

/// Removes every Ghadd section from a hosts file.
pub fn strip_section(hosts: &str) -> String {
    let mut out = hosts.to_string();
    while let Some((s, e)) = section_range(&out) {
        out.replace_range(s..e, "");
    }
    // Drop the blank separator line that `splice` added.
    out.truncate(out.trim_end_matches('\n').len());
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Replaces any existing Ghadd section with `section`, appended at the end.
pub fn splice(hosts: &str, section: &str) -> String {
    let mut base = strip_section(hosts);
    base.truncate(base.trim_end_matches('\n').len());
    if !base.is_empty() {
        base.push_str("\n\n");
    }
    base.push_str(section);
    base
}

/// Stable FNV-1a hash, used to notice when the hosts section was changed behind our back.
pub fn fingerprint(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hosts_and_plain_formats() {
        let text = "# comment\n0.0.0.0 0.0.0.0\n127.0.0.1 localhost\n0.0.0.0 Bad.COM # tail\n\
                    0.0.0.0 a.bad.com b.bad.com\nplain.example\n93.184.216.34 real.example\n";
        let got: Vec<_> = parse_list(text).into_iter().collect();
        assert_eq!(got, ["a.bad.com", "b.bad.com", "bad.com", "plain.example"]);
    }

    #[test]
    fn categories_parse() {
        let manga = category_domains(category("manga-anime").unwrap());
        assert!(manga.contains("toongod.org") && manga.contains("www.toongod.org"));
        assert!(manga.len() > 200);
    }

    #[test]
    fn rejects_bad_domains() {
        for bad in ["nodot", "-x.com", "a..b", "1.2.3.4", "sp ace.com", "localhost.localdomain"] {
            assert_eq!(normalize_domain(bad), None, "{bad}");
        }
        assert_eq!(normalize_domain(" Example.com. ").as_deref(), Some("example.com"));
    }

    #[test]
    fn splice_is_idempotent_and_replaces() {
        let hosts = "127.0.0.1 localhost\n";
        let one = render_section(&parse_list("x.com"), &[]);
        let two = render_section(&parse_list("x.com\ny.com"), &[]);
        let a = splice(hosts, &one);
        assert_eq!(splice(&a, &one), a);
        let b = splice(&a, &two);
        assert_eq!(extract_section(&b), Some(two.as_str()));
        assert_eq!(b.matches(BEGIN_MARKER).count(), 1);
        assert!(b.starts_with(hosts));
        assert_eq!(strip_section(&b).trim_end(), hosts.trim_end());
    }

    #[test]
    fn recovers_from_truncated_section() {
        let hosts = format!("127.0.0.1 localhost\n{BEGIN_MARKER}\n0.0.0.0 x.com\n");
        let section = render_section(&parse_list("y.com"), &[]);
        let fixed = splice(&hosts, &section);
        assert_eq!(fixed, format!("127.0.0.1 localhost\n\n{section}"));
    }
}
