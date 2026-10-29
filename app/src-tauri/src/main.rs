//! Ghadd desktop app: streak tracker, reminders, SOS screen and blocker status.

mod content;
mod db;
mod reminders;

use content::{Content, Item};
use db::{Db, Event, Settings};
use ghadd_core::config::DEFAULT_SOCKET_PATH;
use ghadd_core::ipc::{self, Request, Response, StatusInfo};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

pub struct AppState {
    db: Mutex<Db>,
    content: Content,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Serialize)]
struct Snapshot {
    now: i64,
    journey_start: Option<i64>,
    relapses: Vec<Event>,
    urges: Vec<Event>,
    settings: Settings,
    autostart: bool,
}

#[tauri::command]
fn snapshot(app: AppHandle, state: State<AppState>) -> CmdResult<Snapshot> {
    let db = state.db.lock().unwrap();
    Ok(Snapshot {
        now: now(),
        journey_start: db.journey_start().map_err(err)?,
        relapses: db.relapses().map_err(err)?,
        urges: db.urges().map_err(err)?,
        settings: db.settings().map_err(err)?,
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
    })
}

#[tauri::command]
fn start_journey(state: State<AppState>, ts: i64) -> CmdResult<()> {
    state.db.lock().unwrap().set_journey_start(ts.min(now())).map_err(err)
}

#[tauri::command]
fn log_relapse(state: State<AppState>, note: String) -> CmdResult<()> {
    state.db.lock().unwrap().add_relapse(now(), note.trim()).map_err(err)
}

#[tauri::command]
fn log_urge(state: State<AppState>, outcome: String) -> CmdResult<()> {
    state.db.lock().unwrap().add_urge(now(), &outcome).map_err(err)
}

#[tauri::command]
fn pick_content(state: State<AppState>, kind: String) -> Item {
    state.content.pick(&kind)
}

#[tauri::command]
fn ayah(state: State<AppState>, reference: String) -> Option<Item> {
    state.content.ayah(&reference)
}

#[tauri::command]
fn dhikr(state: State<AppState>) -> Vec<String> {
    state.content.dhikr.clone()
}

#[tauri::command]
fn save_settings(state: State<AppState>, settings: Settings) -> CmdResult<()> {
    state.db.lock().unwrap().save_settings(&settings).map_err(err)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<()> {
    let launcher = app.autolaunch();
    if enabled { launcher.enable() } else { launcher.disable() }.map_err(err)
}

#[tauri::command]
fn test_notification(app: AppHandle) {
    reminders::send_reminder(&app);
}

/// Talks to ghaddd off the main thread (a wrong password makes it wait).
async fn daemon(req: Request) -> CmdResult<Response> {
    tauri::async_runtime::spawn_blocking(move || ipc::send(Path::new(DEFAULT_SOCKET_PATH), &req))
        .await
        .map_err(err)?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn blocker_status() -> CmdResult<StatusInfo> {
    match daemon(Request::Status).await? {
        Response::Status(s) => Ok(s),
        Response::Error { message } => Err(message),
        _ => Err("unexpected reply from ghaddd".into()),
    }
}

#[tauri::command]
async fn blocker_add(domain: String) -> CmdResult<String> {
    match daemon(Request::AddDomain { domain }).await? {
        Response::Ok { message } => Ok(message),
        Response::Error { message } => Err(message),
        _ => Err("unexpected reply from ghaddd".into()),
    }
}

#[tauri::command]
async fn blocker_category_on(name: String) -> CmdResult<String> {
    match daemon(Request::EnableCategory { name }).await? {
        Response::Ok { message } => Ok(message),
        Response::Error { message } => Err(message),
        _ => Err("unexpected reply from ghaddd".into()),
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Ghadd", true, None::<&str>)?;
    let sos = MenuItem::with_id(app, "sos", "SOS: I feel an urge", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit (blocking stays on)", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &sos, &PredefinedMenuItem::separator(app)?, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("app has an icon"))
        .tooltip("Ghadd")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "sos" => {
                show_main(app);
                let _ = app.emit("open-sos", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        // Must be first: a second launch (e.g. from the app menu) just shows the running window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = Db::open(&dir.join("ghadd.db"))?;
            app.manage(AppState { db: Mutex::new(db), content: Content::load() });
            build_tray(app)?;
            if !std::env::args().any(|a| a == "--hidden") {
                show_main(app.handle());
            }
            reminders::spawn(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps reminders running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            start_journey,
            log_relapse,
            log_urge,
            pick_content,
            ayah,
            dhikr,
            save_settings,
            set_autostart,
            test_notification,
            blocker_status,
            blocker_add,
            blocker_category_on,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ghadd");
}
