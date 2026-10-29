//! Background loop: periodic reminder notifications, streak milestones and the tray tooltip.

use crate::AppState;
use chrono::{Local, Timelike};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

/// Days worth celebrating (40 days is a well-known turning point for habits).
const MILESTONES: &[i64] = &[1, 3, 7, 14, 21, 30, 40, 60, 90, 120, 180, 270, 365, 500, 730, 1000];
const TICK: Duration = Duration::from_secs(60);

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_reminder = Instant::now();
        loop {
            if let Err(e) = tick(&app, &mut last_reminder) {
                eprintln!("reminder tick failed: {e:#}");
            }
            std::thread::sleep(TICK);
        }
    });
}

fn tick(app: &AppHandle, last_reminder: &mut Instant) -> anyhow::Result<()> {
    let state = app.state::<AppState>();
    let now = chrono::Utc::now().timestamp();
    let (settings, streak_start, celebrated) = {
        let db = state.db.lock().unwrap();
        let start = db.streak_start()?;
        let celebrated = match start {
            Some(s) => db.meta(&format!("celebrated:{s}"))?.and_then(|v| v.parse().ok()).unwrap_or(0),
            None => 0,
        };
        (db.settings()?, start, celebrated)
    };

    if let Some(start) = streak_start {
        let days = (now - start) / 86400;
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(format!("Ghadd · {days} days clean")));
        }
        // Only the highest milestone reached, so setting an old start date doesn't spam.
        if let Some(&m) = MILESTONES.iter().rev().find(|&&m| m <= days && m > celebrated) {
            notify(app, &format!("🎉 {m} days clean"), "ما شاء الله! Keep going, may Allah keep you firm.");
            state.db.lock().unwrap().set_meta(&format!("celebrated:{start}"), &m.to_string())?;
        }
    }

    let interval = Duration::from_secs(u64::from(settings.interval_min.max(5)) * 60);
    if settings.reminders && last_reminder.elapsed() >= interval {
        if !in_quiet_hours(Local::now().hour(), settings.quiet_start, settings.quiet_end) {
            send_reminder(app);
        }
        *last_reminder = Instant::now();
    }
    Ok(())
}

pub fn send_reminder(app: &AppHandle) {
    let item = app.state::<AppState>().content.pick("any");
    let title = match item.kind {
        "quran" => "📖 آية",
        "hadith" => "🤍 حديث",
        _ => "🌿 تذكير",
    };
    let body = if item.source.is_empty() { item.text } else { format!("{}\n— {}", item.text, item.source) };
    notify(app, title, &body);
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("notification failed: {e}");
    }
}

fn in_quiet_hours(hour: u32, start: u32, end: u32) -> bool {
    match start.cmp(&end) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Less => (start..end).contains(&hour),
        std::cmp::Ordering::Greater => hour >= start || hour < end,
    }
}

#[cfg(test)]
mod tests {
    use super::in_quiet_hours;

    #[test]
    fn quiet_hours_wrap_midnight() {
        assert!(in_quiet_hours(23, 23, 7));
        assert!(in_quiet_hours(3, 23, 7));
        assert!(!in_quiet_hours(7, 23, 7));
        assert!(!in_quiet_hours(12, 23, 7));
        assert!(in_quiet_hours(13, 13, 15));
        assert!(!in_quiet_hours(13, 9, 9));
    }
}
