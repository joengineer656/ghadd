//! ghaddd — the Ghadd blocker daemon. Runs as root, keeps the blocklist applied
//! to the hosts file, and answers the app and CLI over a Unix socket.

mod daemon;
mod state;
mod system;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use daemon::{fetch_lists, Daemon};
use ghadd_core::config::{Config, DEFAULT_CONFIG_PATH};
use ghadd_core::ipc::{Request, Response};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{fs, thread};

const LIST_RETRY_INTERVAL: Duration = Duration::from_secs(10 * 60);

#[derive(Parser)]
#[command(version, about = "Ghadd blocker daemon")]
struct Args {
    #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the daemon (default)
    Run,
    /// Apply the blocklist once and exit
    EnforceOnce,
    /// Download the blocklists, apply them and exit
    UpdateLists,
    /// Undo all system changes (run by the package on uninstall)
    Cleanup,
}

type Shared = Arc<Mutex<Daemon>>;

fn main() -> Result<()> {
    let args = Args::parse();
    let cfg = Config::load(&args.config)?;
    let mut daemon = Daemon::load(cfg)?;
    match args.command.unwrap_or(Cmd::Run) {
        Cmd::EnforceOnce => daemon.enforce(),
        Cmd::Cleanup => daemon.cleanup(),
        Cmd::UpdateLists => {
            let (domains, complete) = fetch_lists(&daemon.cfg.sources)?;
            daemon.set_lists(domains, complete)
        }
        Cmd::Run => run(Arc::new(Mutex::new(daemon))),
    }
}

fn run(daemon: Shared) -> Result<()> {
    let (socket, interval) = {
        let mut d = daemon.lock().unwrap();
        if let Err(e) = d.enforce() {
            eprintln!("error: initial enforce failed: {e:#}");
        }
        (d.cfg.socket_path.clone(), Duration::from_secs(d.cfg.check_interval_secs.max(5)))
    };

    let watchdog = daemon.clone();
    thread::spawn(move || watchdog_loop(watchdog, interval));

    let _ = fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).with_context(|| format!("cannot bind {}", socket.display()))?;
    // Any local user may talk to the daemon; everything that weakens the block needs the friend password.
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o666))?;
    eprintln!("ghaddd listening on {}", socket.display());

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let daemon = daemon.clone();
        thread::spawn(move || {
            if let Err(e) = handle_conn(&daemon, stream) {
                eprintln!("connection error: {e:#}");
            }
        });
    }
    Ok(())
}

/// Repairs the hosts file every `interval` and keeps the blocklists fresh.
fn watchdog_loop(daemon: Shared, interval: Duration) {
    let mut last_list_attempt: Option<Instant> = None;
    loop {
        let stale = {
            let mut d = daemon.lock().unwrap();
            if let Err(e) = d.enforce() {
                eprintln!("error: enforce failed: {e:#}");
            }
            d.lists_stale()
        };
        if stale && last_list_attempt.map_or(true, |t| t.elapsed() >= LIST_RETRY_INTERVAL) {
            last_list_attempt = Some(Instant::now());
            if let Response::Error { message } = update_lists(&daemon) {
                eprintln!("error: list update failed: {message}");
            }
        }
        thread::sleep(interval);
    }
}

/// Downloads without holding the lock, so the socket stays responsive.
fn update_lists(daemon: &Shared) -> Response {
    let sources = daemon.lock().unwrap().cfg.sources.clone();
    let result = fetch_lists(&sources).and_then(|(domains, complete)| {
        let count = domains.len();
        daemon.lock().unwrap().set_lists(domains, complete)?;
        Ok(count)
    });
    match result {
        Ok(count) => Response::Ok { message: format!("blocklists updated ({count} domains)") },
        Err(e) => Response::Error { message: format!("{e:#}") },
    }
}

fn handle_conn(daemon: &Shared, stream: UnixStream) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut line = String::new();
    BufReader::new((&stream).take(64 * 1024)).read_line(&mut line)?;
    let reply = match serde_json::from_str::<Request>(&line) {
        Ok(Request::UpdateLists) => update_lists(daemon),
        Ok(req) => daemon.lock().unwrap().handle(req),
        Err(e) => Response::Error { message: format!("bad request: {e}") },
    };
    let mut out = serde_json::to_string(&reply)?;
    out.push('\n');
    (&stream).write_all(out.as_bytes())?;
    Ok(())
}
