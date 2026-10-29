"""Screenshots of the Ghadd app for the docs, rendered with WebKitGTK (the engine
the app itself uses). The Rust backend is replaced by a mock with sample data.

    python3 docs/shots/render.py            # writes docs/img/*.png
"""
import os
import shutil
import sys
import tempfile
from pathlib import Path

import gi

# Offscreen windows have no GL context; render in software.
os.environ.setdefault("WEBKIT_DISABLE_COMPOSITING_MODE", "1")
os.environ.setdefault("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
os.environ.setdefault("GDK_BACKEND", "x11")

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs" / "img"

# (file name, state, width, height, full page, dark theme)
SHOTS = [
    ("dashboard", "", 1100, 900, True, False),
    ("dashboard-dark", "", 1100, 900, True, True),
    ("onboarding", "onboard", 1000, 640, False, False),
    ("relapse", "relapse", 1000, 760, False, False),
    ("sos", "sos", 1000, 1050, False, True),
    ("reading", "reading", 420, 330, False, True),
]

MOCK = """
(() => {
  const CONTENT = __CONTENT__;
  const state = location.hash.slice(1);
  const now = Math.floor(Date.now() / 1000);
  const DAY = 86400;
  const journey = now - 61 * DAY;
  const snapshot = {
    now,
    journey_start: state === "onboard" ? null : journey,
    relapses: [{ ts: journey + 14 * DAY, note: "" }, { ts: now - 23 * DAY - 5 * 3600, note: "late night, alone" }],
    urges: [3, 9, 10, 20, 27, 33, 41, 45, 52, 58].map((d) => ({ ts: journey + d * DAY, note: "resisted" })),
    settings: { reminders: true, interval_min: 120, quiet_start: 23, quiet_end: 7 },
    autostart: true,
  };
  const ayah = (ref) => {
    const a = CONTENT.quran.find((q) => q.ref === ref);
    return { kind: "quran", text: a.text, source: `سورة ${a.surah} ${a.ref}` };
  };
  const pick = (kind) => {
    if (kind === "sos") return ayah("79:40-41");
    if (kind === "quran") return ayah("13:28");
    if (kind === "reminder") return { kind, text: CONTENT.reminders[0], source: "" };
    const h = CONTENT.hadith.find((h) => h.source.includes("2822"));
    return { kind: "hadith", text: h.text, source: h.source };
  };
  const handlers = {
    snapshot: () => snapshot,
    pick_content: ({ kind }) => pick(kind),
    ayah: ({ reference }) => ayah(reference),
    dhikr: () => CONTENT.dhikr,
    blocker_status: () => ({
      active: true, disabled: false, paused_until: null, password_set: true,
      blocked_count: 116122, extra_domains: [], allowed_domains: [], categories: ["manga-anime"],
      last_list_update: now - 3 * 3600, tamper_count: 0,
    }),
  };
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => (handlers[cmd] ? handlers[cmd](args || {}) : null) },
    event: { listen: async () => () => {} },
  };
  window.addEventListener("load", () => setTimeout(() => {
    if (state === "relapse") document.getElementById("relapse-btn").click();
    if (state === "sos") {
      document.getElementById("sos-btn").click();
      setTimeout(() => { for (let i = 0; i < 12; i++) document.getElementById("dhikr-btn").click(); }, 300);
    }
    if (state === "reading") {
      document.querySelector("header").remove();
      for (const c of document.querySelectorAll(".card:not(.reading)")) c.remove();
    }
  }, 200));
})();
"""


def build_site(tmp: Path) -> Path:
    site = tmp / "site"
    shutil.copytree(ROOT / "app" / "src", site)
    content = (ROOT / "data" / "content.json").read_text()
    (site / "mock.js").write_text(MOCK.replace("__CONTENT__", content))
    html = (site / "index.html").read_text()
    html = html.replace('<script type="module" src="main.js">', '<script src="mock.js"></script>\n  <script type="module" src="main.js">')
    (site / "index.html").write_text(html)
    return site / "index.html"


def shoot(index: Path, name, state, width, height, full, dark):
    Gtk.Settings.get_default().set_property("gtk-application-prefer-dark-theme", dark)
    window = Gtk.OffscreenWindow()
    view = WebKit2.WebView()
    settings = view.get_settings()
    settings.set_allow_file_access_from_file_urls(True)
    settings.set_allow_universal_access_from_file_urls(True)
    settings.set_hardware_acceleration_policy(WebKit2.HardwareAccelerationPolicy.NEVER)
    view.set_size_request(width, height)
    window.add(view)
    window.show_all()

    loop = GLib.MainLoop()

    def snapped(v, result):
        surface = v.get_snapshot_finish(result)
        surface.write_to_png(str(OUT / f"{name}.png"))
        print(f"wrote docs/img/{name}.png")
        loop.quit()

    def take():
        region = WebKit2.SnapshotRegion.FULL_DOCUMENT if full else WebKit2.SnapshotRegion.VISIBLE
        view.get_snapshot(region, WebKit2.SnapshotOptions.NONE, None, snapped)
        return False

    def loaded(v, event):
        if event == WebKit2.LoadEvent.FINISHED:
            GLib.timeout_add(1800, take)

    view.connect("load-changed", loaded)
    view.load_uri(index.as_uri() + (f"#{state}" if state else ""))
    loop.run()
    window.destroy()


def main():
    only = set(sys.argv[1:])
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        index = build_site(Path(tmp))
        for shot in SHOTS:
            if not only or shot[0] in only:
                shoot(index, *shot)


main()
