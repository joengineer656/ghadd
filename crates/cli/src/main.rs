//! ghadd — command-line client for the Ghadd blocker daemon.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use ghadd_core::config::DEFAULT_SOCKET_PATH;
use ghadd_core::blocklist;
use ghadd_core::ipc::{self, Request, Response};
use ghadd_core::unix_now;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Control the Ghadd website blocker")]
struct Args {
    #[arg(long, default_value = DEFAULT_SOCKET_PATH)]
    socket: PathBuf,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show whether blocking is on
    Status,
    /// Block another site (no password needed)
    Add { domain: String },
    /// Unblock a site (needs the friend password)
    Remove { domain: String },
    /// List built-in categories, or switch one on/off (off needs the friend password)
    Category {
        #[command(subcommand)]
        action: Option<CategoryCmd>,
    },
    /// Turn blocking off for a while (needs the friend password)
    Pause { minutes: u64 },
    /// End a pause early
    Resume,
    /// Turn blocking off until enabled again (needs the friend password)
    Disable,
    /// Turn blocking back on
    Enable,
    /// Set or change the friend password
    SetPassword,
    /// Download the latest blocklists now
    UpdateLists,
    /// Show attempts to tamper with the block
    TamperLog,
}

#[derive(Subcommand)]
enum CategoryCmd {
    /// Start blocking a category, e.g. `ghadd category on manga-anime`
    On { name: String },
    /// Stop blocking a category (needs the friend password)
    Off { name: String },
}

fn main() -> Result<()> {
    let args = Args::parse();
    let req = match args.command {
        Cmd::Category { action: None } => return list_categories(&args.socket),
        Cmd::Category { action: Some(CategoryCmd::On { name }) } => Request::EnableCategory { name },
        Cmd::Category { action: Some(CategoryCmd::Off { name }) } => {
            Request::DisableCategory { name, password: friend_password()? }
        }
        Cmd::Status => Request::Status,
        Cmd::Add { domain } => Request::AddDomain { domain },
        Cmd::Remove { domain } => Request::RemoveDomain { domain, password: friend_password()? },
        Cmd::Pause { minutes } => Request::Pause { minutes, password: friend_password()? },
        Cmd::Resume => Request::Resume,
        Cmd::Disable => Request::Disable { password: friend_password()? },
        Cmd::Enable => Request::Enable,
        Cmd::SetPassword => set_password_request(&args.socket)?,
        Cmd::UpdateLists => Request::UpdateLists,
        Cmd::TamperLog => Request::TamperEvents,
    };
    match ipc::send(&args.socket, &req)? {
        Response::Ok { message } => println!("{message}"),
        Response::Status(s) => {
            let state = match (s.active, s.disabled, s.paused_until) {
                (true, _, _) => "ON".to_string(),
                (_, true, _) => "DISABLED".to_string(),
                (_, _, Some(t)) => format!("PAUSED ({} min left)", t.saturating_sub(unix_now()).div_ceil(60)),
                _ => "OFF".to_string(),
            };
            println!("Blocking:         {state}");
            println!("Blocked domains:  {}", s.blocked_count);
            println!("Friend password:  {}", if s.password_set { "set" } else { "NOT SET" });
            println!("Lists updated:    {}", s.last_list_update.map_or("never".into(), ago));
            println!("Tamper events:    {}", s.tamper_count);
            if !s.extra_domains.is_empty() {
                println!("Your extra sites: {}", s.extra_domains.join(", "));
            }
            if !s.categories.is_empty() {
                println!("Categories:       {}", s.categories.join(", "));
            }
            if !s.allowed_domains.is_empty() {
                println!("Unblocked sites:  {}", s.allowed_domains.join(", "));
            }
        }
        Response::TamperEvents { events } if events.is_empty() => println!("No tampering recorded."),
        Response::TamperEvents { events } => {
            for e in events {
                println!("{:>12}  {}", ago(e.ts), e.detail);
            }
        }
        Response::Error { message } => bail!(message),
    }
    Ok(())
}

fn list_categories(socket: &std::path::Path) -> Result<()> {
    let on = match ipc::send(socket, &Request::Status)? {
        Response::Status(s) => s.categories,
        _ => Vec::new(),
    };
    for c in blocklist::CATEGORIES {
        let state = if on.iter().any(|n| n == c.name) { "ON " } else { "off" };
        let count = blocklist::category_domains(c).len();
        println!("[{state}] {:<14} {} ({count} domains)", c.name, c.title);
    }
    Ok(())
}

fn friend_password() -> Result<String> {
    Ok(rpassword::prompt_password("Friend password: ")?)
}

fn set_password_request(socket: &std::path::Path) -> Result<Request> {
    let password_set = matches!(ipc::send(socket, &Request::Status)?, Response::Status(s) if s.password_set);
    let old = if password_set {
        Some(rpassword::prompt_password("Current friend password: ")?)
    } else {
        None
    };
    println!("Let your friend type the new password; don't watch, and don't write it down.");
    let new = rpassword::prompt_password("New friend password: ")?;
    if new != rpassword::prompt_password("Repeat it: ")? {
        bail!("the passwords don't match");
    }
    Ok(Request::SetPassword { old, new })
}

fn ago(ts: u64) -> String {
    let secs = unix_now().saturating_sub(ts);
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86399 => format!("{} h ago", secs / 3600),
        _ => format!("{} days ago", secs / 86400),
    }
}
