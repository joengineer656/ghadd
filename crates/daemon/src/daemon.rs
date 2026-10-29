//! The blocker itself: keeps the hosts file in the state the user (and friend) agreed on.

use crate::state::State;
use crate::system::{self, Hook, ManagedFile};
use anyhow::{anyhow, bail, ensure, Context, Result};
use ghadd_core::ipc::{Request, Response, StatusInfo};
use ghadd_core::{blocklist, config::Config, lock, unix_now};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

const LIST_MAX_AGE_SECS: u64 = 7 * 24 * 3600;
const MAX_PAUSE_MINUTES: u64 = 24 * 60;
const WRONG_PASSWORD_DELAY: Duration = Duration::from_secs(3);

pub struct Daemon {
    pub cfg: Config,
    state: State,
    /// Domains from the downloaded blocklists.
    lists: BTreeSet<String>,
}

impl Daemon {
    pub fn load(cfg: Config) -> Result<Self> {
        fs::create_dir_all(&cfg.state_dir)
            .with_context(|| format!("cannot create {}", cfg.state_dir.display()))?;
        let state = State::load(&cfg.state_dir.join("state.json"))?;
        let lists = fs::read_to_string(cfg.state_dir.join("lists.txt"))
            .map(|t| blocklist::parse_list(&t))
            .unwrap_or_default();
        Ok(Self { cfg, state, lists })
    }

    fn save(&self) -> Result<()> {
        self.state.save(&self.cfg.state_dir.join("state.json"))
    }

    fn is_active(&self) -> bool {
        !self.state.disabled && self.state.paused_until.map_or(true, |t| unix_now() >= t)
    }

    fn desired_domains(&self) -> BTreeSet<String> {
        let pinned = system::pinned_hosts(&self.cfg);
        let categories = self
            .state
            .categories
            .iter()
            .filter_map(|name| blocklist::category(name))
            .flat_map(blocklist::category_domains);
        self.lists
            .union(&self.state.extra_domains)
            .cloned()
            .chain(categories)
            .filter(|d| !self.state.allowed_domains.contains(d))
            .chain(system::DOH_HOSTS.iter().map(|h| h.to_string()))
            .filter(|d| !pinned.iter().any(|(_, host)| host == d))
            .collect()
    }

    pub fn lists_stale(&self) -> bool {
        self.state.last_list_update.map_or(true, |t| unix_now().saturating_sub(t) > LIST_MAX_AGE_SECS)
    }

    /// Brings the hosts file, DNS config and browser policies in line with the
    /// current state, logging any tampering.
    pub fn enforce(&mut self) -> Result<()> {
        self.enforce_as(self.is_active())
    }

    /// Removes everything ghaddd put on the system (used when uninstalling).
    /// The saved state is kept, so reinstalling restores the block.
    pub fn cleanup(&mut self) -> Result<()> {
        self.enforce_as(false)?;
        set_immutable(&self.cfg.hosts_path, false);
        Ok(())
    }

    fn enforce_as(&mut self, active: bool) -> Result<()> {
        let mut changed = false;
        let mut hooks = BTreeSet::new();

        if self.enforce_hosts(active, &mut changed)? {
            hooks.insert(Hook::FlushDns);
        }
        for file in system::managed_files(&self.cfg, active) {
            match self.enforce_file(&file, &mut changed) {
                Ok(true) => {
                    hooks.extend(file.hook);
                    hooks.insert(Hook::FlushDns);
                }
                Ok(false) => {}
                Err(e) => eprintln!("error: {}: {e:#}", file.path.display()),
            }
        }
        if self.cfg.run_hooks {
            system::run_hooks(&hooks);
            if active && self.cfg.manage_dns {
                for link in system::revert_link_dns() {
                    eprintln!("reset DNS on {link}: it was using its own DNS server instead of the filter");
                }
            }
        }

        if self.state.paused_until.is_some_and(|t| unix_now() >= t) {
            self.state.paused_until = None;
            changed = true;
        }
        if changed {
            self.save()?;
        }
        Ok(())
    }

    /// Records tampering if `current` isn't what we last wrote to `path`.
    fn check_tamper(&mut self, path: &Path, current: Option<&str>) -> bool {
        let key = path.display().to_string();
        match self.state.applied.get(&key) {
            Some(&fp) if current.map(blocklist::fingerprint) != Some(fp) => {
                self.state.record_tamper(unix_now(), format!("{key} was edited or removed"));
                true
            }
            _ => false,
        }
    }

    /// Returns whether the hosts file was rewritten.
    fn enforce_hosts(&mut self, active: bool, changed: &mut bool) -> Result<bool> {
        let path = self.cfg.hosts_path.clone();
        let key = path.display().to_string();
        let hosts = fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let current = blocklist::extract_section(&hosts);
        *changed |= self.check_tamper(&path, current);

        let new_hosts = if active {
            let section = blocklist::render_section(&self.desired_domains(), &system::pinned_hosts(&self.cfg));
            let fp = blocklist::fingerprint(&section);
            *changed |= self.state.applied.insert(key, fp) != Some(fp);
            (current != Some(section.as_str())).then(|| blocklist::splice(&hosts, &section))
        } else {
            *changed |= self.state.applied.remove(&key).is_some();
            current.is_some().then(|| blocklist::strip_section(&hosts))
        };

        if let Some(text) = &new_hosts {
            write_file(&path, text, self.cfg.make_immutable)?;
        }
        if self.cfg.make_immutable {
            // Re-locks the file if someone cleared the flag without editing it.
            set_immutable(&path, true);
        }
        Ok(new_hosts.is_some())
    }

    /// Writes or removes one managed file. Returns whether it changed on disk.
    /// A file that existed before we first took it over is backed up and
    /// restored when we let go of it.
    fn enforce_file(&mut self, file: &ManagedFile, changed: &mut bool) -> Result<bool> {
        let key = file.path.display().to_string();
        let current = fs::read_to_string(&file.path).ok();
        *changed |= self.check_tamper(&file.path, current.as_deref());
        let backup = file.path.with_extension("ghadd-backup");
        let immutable = self.cfg.make_immutable;

        let Some(want) = &file.content else {
            if self.state.applied.remove(&key).is_none() {
                return Ok(false); // never ours, leave it alone
            }
            *changed = true;
            if current.is_some() {
                set_immutable(&file.path, false);
                fs::remove_file(&file.path)?;
            }
            if backup.exists() {
                fs::rename(&backup, &file.path)?;
            }
            return Ok(true);
        };

        let fp = blocklist::fingerprint(want);
        let was_ours = self.state.applied.insert(key, fp).is_some();
        *changed |= !was_ours;
        if current.as_deref() == Some(want.as_str()) {
            if immutable {
                set_immutable(&file.path, true);
            }
            return Ok(false);
        }
        if current.is_some() && !was_ours {
            fs::copy(&file.path, &backup)?;
        }
        write_file(&file.path, want, immutable)?;
        if immutable {
            set_immutable(&file.path, true);
        }
        Ok(true)
    }

    /// Replaces the downloaded lists. A partial download is merged into the old
    /// lists instead, so one dead mirror never weakens the block.
    pub fn set_lists(&mut self, domains: BTreeSet<String>, complete: bool) -> Result<()> {
        if complete {
            self.lists = domains;
        } else {
            self.lists.extend(domains);
        }
        let text: String = self.lists.iter().map(|d| format!("{d}\n")).collect();
        fs::write(self.cfg.state_dir.join("lists.txt"), text)?;
        self.state.last_list_update = Some(unix_now());
        self.save()?;
        self.enforce()
    }

    pub fn handle(&mut self, req: Request) -> Response {
        self.try_handle(req)
            .unwrap_or_else(|e| Response::Error { message: format!("{e:#}") })
    }

    fn try_handle(&mut self, req: Request) -> Result<Response> {
        let message = match req {
            Request::Status => return Ok(Response::Status(self.status())),
            Request::TamperEvents => {
                return Ok(Response::TamperEvents { events: self.state.tamper_events.clone() })
            }
            Request::UpdateLists => bail!("update_lists is handled by the server"),
            Request::AddDomain { domain } => {
                let domains = with_www(&domain)?;
                for d in &domains {
                    self.state.allowed_domains.remove(d);
                    self.state.extra_domains.insert(d.clone());
                }
                format!("now blocking {}", domains.join(", "))
            }
            Request::RemoveDomain { domain, password } => {
                self.check_password(&password)?;
                let domains = with_www(&domain)?;
                let in_category = |d: &String| {
                    self.state.categories.iter().filter_map(|n| blocklist::category(n)).any(|c| {
                        blocklist::category_domains(c).contains(d)
                    })
                };
                let from_lists: Vec<_> =
                    domains.iter().filter(|d| self.lists.contains(*d) || in_category(d)).cloned().collect();
                for d in &domains {
                    self.state.extra_domains.remove(d);
                }
                self.state.allowed_domains.extend(from_lists);
                format!("unblocked {}", domains.join(", "))
            }
            Request::EnableCategory { name } => {
                let c = blocklist::category(&name).ok_or_else(|| anyhow!("unknown category '{name}'"))?;
                // Re-blocks anything from this category that was unblocked before.
                for d in blocklist::category_domains(c) {
                    self.state.allowed_domains.remove(&d);
                }
                self.state.categories.insert(name);
                format!("now blocking {}", c.title)
            }
            Request::DisableCategory { name, password } => {
                self.check_password(&password)?;
                let c = blocklist::category(&name).ok_or_else(|| anyhow!("unknown category '{name}'"))?;
                self.state.categories.remove(&name);
                format!("stopped blocking {}", c.title)
            }
            Request::Pause { minutes, password } => {
                self.check_password(&password)?;
                ensure!((1..=MAX_PAUSE_MINUTES).contains(&minutes), "pause must be 1–{MAX_PAUSE_MINUTES} minutes");
                self.state.paused_until = Some(unix_now() + minutes * 60);
                format!("blocking paused for {minutes} min, it turns back on by itself")
            }
            Request::Resume => {
                self.state.paused_until = None;
                "blocking resumed".into()
            }
            Request::Disable { password } => {
                self.check_password(&password)?;
                self.state.disabled = true;
                "blocking disabled".into()
            }
            Request::Enable => {
                self.state.disabled = false;
                self.state.paused_until = None;
                "blocking enabled".into()
            }
            Request::SetPassword { old, new } => {
                if self.read_lock_hash()?.is_some() {
                    self.check_password(old.as_deref().unwrap_or(""))?;
                }
                let hash = lock::hash_password(&new)?;
                write_private(&self.cfg.lock_path, &hash)?;
                "friend password saved".into()
            }
        };
        self.save()?;
        self.enforce()?;
        Ok(Response::Ok { message })
    }

    fn status(&self) -> StatusInfo {
        StatusInfo {
            active: self.is_active(),
            disabled: self.state.disabled,
            paused_until: self.state.paused_until.filter(|&t| t > unix_now()),
            password_set: self.read_lock_hash().ok().flatten().is_some(),
            blocked_count: self.desired_domains().len(),
            extra_domains: self.state.extra_domains.iter().cloned().collect(),
            allowed_domains: self.state.allowed_domains.iter().cloned().collect(),
            categories: self.state.categories.iter().cloned().collect(),
            last_list_update: self.state.last_list_update,
            tamper_count: self.state.tamper_events.len(),
        }
    }

    fn read_lock_hash(&self) -> Result<Option<String>> {
        match fs::read_to_string(&self.cfg.lock_path) {
            Ok(h) if !h.trim().is_empty() => Ok(Some(h.trim().to_string())),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).context("cannot read the friend password file"),
        }
    }

    fn check_password(&self, password: &str) -> Result<()> {
        let hash = self
            .read_lock_hash()?
            .ok_or_else(|| anyhow!("no friend password is set yet; set one first"))?;
        if !lock::verify_password(password, &hash) {
            // Callers hold the daemon lock, so this also throttles guessing.
            std::thread::sleep(WRONG_PASSWORD_DELAY);
            bail!("wrong friend password");
        }
        Ok(())
    }
}

/// Downloads every source. Returns the merged domains and whether all sources succeeded.
pub fn fetch_lists(sources: &[String]) -> Result<(BTreeSet<String>, bool)> {
    let mut all = BTreeSet::new();
    let mut ok = 0;
    for url in sources {
        match fetch(url) {
            Ok(text) => {
                all.extend(blocklist::parse_list(&text));
                ok += 1;
            }
            Err(e) => eprintln!("warning: could not download {url}: {e:#}"),
        }
    }
    ensure!(ok > 0, "no blocklist could be downloaded");
    Ok((all, ok == sources.len()))
}

fn fetch(url: &str) -> Result<String> {
    let mut text = String::new();
    ureq::get(url)
        .timeout(Duration::from_secs(120))
        .call()?
        .into_reader()
        .take(200 << 20)
        .read_to_string(&mut text)?;
    Ok(text)
}

/// The domain plus its `www.` twin, which blocklists usually list separately.
fn with_www(raw: &str) -> Result<Vec<String>> {
    let d = blocklist::normalize_domain(raw).ok_or_else(|| anyhow!("'{raw}' is not a valid domain"))?;
    let bare = d.strip_prefix("www.").unwrap_or(&d).to_string();
    Ok(vec![format!("www.{bare}"), bare])
}

fn write_file(path: &Path, text: &str, immutable: bool) -> Result<()> {
    if immutable {
        set_immutable(path, false);
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("ghadd-tmp");
    fs::write(&tmp, text)?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o644))?;
    fs::rename(&tmp, path).with_context(|| format!("cannot write {}", path.display()))
}

fn write_private(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp: PathBuf = path.with_extension("tmp");
    fs::write(&tmp, text)?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    fs::rename(&tmp, path).with_context(|| format!("cannot write {}", path.display()))
}

// From <linux/fs.h>; the ioctls take an int even though the macros say long.
const FS_IOC_GETFLAGS: libc::c_ulong = 0x8008_6601;
const FS_IOC_SETFLAGS: libc::c_ulong = 0x4008_6602;
const FS_IMMUTABLE_FL: libc::c_int = 0x10;

/// Sets or clears the immutable flag (`chattr +i`) if it isn't already in that state.
fn set_immutable(path: &Path, on: bool) {
    let result = (|| -> std::io::Result<()> {
        let file = match fs::File::open(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            other => other?,
        };
        let mut flags: libc::c_int = 0;
        // SAFETY: fd is valid for the call and `flags` outlives it.
        if unsafe { libc::ioctl(file.as_raw_fd(), FS_IOC_GETFLAGS as _, &mut flags) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if (flags & FS_IMMUTABLE_FL != 0) == on {
            return Ok(());
        }
        flags ^= FS_IMMUTABLE_FL;
        // SAFETY: as above.
        if unsafe { libc::ioctl(file.as_raw_fd(), FS_IOC_SETFLAGS as _, &flags) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("warning: cannot set immutable={on} on {}: {e}", path.display());
    }
}
