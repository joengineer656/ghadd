const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const DAY = 86400;
const MILESTONES = [1, 3, 7, 14, 21, 30, 40, 60, 90, 120, 180, 270, 365, 500, 730, 1000];
const $ = (id) => document.getElementById(id);

let snap = null;      // last snapshot from the backend
let fetchedAt = 0;    // Date.now() when it arrived, to tick the clock between refreshes

// ---------- data ----------

async function refresh() {
  snap = await invoke("snapshot");
  fetchedAt = Date.now();
  $("onboard").hidden = snap.journey_start != null;
  if (snap.journey_start == null) {
    $("onboard-date").value = toLocalInput(new Date());
    return;
  }
  render();
}

const nowTs = () => snap.now + Math.floor((Date.now() - fetchedAt) / 1000);
const dayKey = (ts) => new Date(ts * 1000).toDateString();

function stats() {
  const start = snap.journey_start;
  const relapses = snap.relapses.map((r) => r.ts).filter((t) => t >= start);
  const points = [start, ...relapses];
  const now = nowTs();
  let best = 0;
  for (let i = 1; i < points.length; i++) best = Math.max(best, points[i] - points[i - 1]);
  const current = now - points[points.length - 1];
  best = Math.max(best, current);

  // Clean days: every calendar day after the start day, up to today, without a relapse.
  const relapseDays = new Set(relapses.map(dayKey));
  let clean = 0;
  const d = new Date(start * 1000);
  d.setHours(12, 0, 0, 0);
  d.setDate(d.getDate() + 1);
  for (; d.getTime() / 1000 <= now; d.setDate(d.getDate() + 1)) {
    if (!relapseDays.has(d.toDateString())) clean++;
  }
  const resisted = snap.urges.filter((u) => u.note === "resisted").length;
  return { current, best, clean, resisted, relapses: relapses.length, relapseDays };
}

// ---------- rendering ----------

function render() {
  const s = stats();
  const days = Math.floor(s.current / DAY);
  $("streak-days").textContent = days;
  $("streak-unit").textContent = days === 1 ? "day" : "days";
  $("streak-time").textContent = `${Math.floor((s.current % DAY) / 3600)} h ${Math.floor((s.current % 3600) / 60)} m`;

  const next = MILESTONES.find((m) => m > days) ?? days + 1;
  const prev = [...MILESTONES].reverse().find((m) => m <= days) ?? 0;
  const progress = (s.current / DAY - prev) / (next - prev);
  $("milestone-fill").style.width = `${Math.min(100, progress * 100)}%`;
  $("milestone-text").textContent = `Next milestone: ${next} days (${next - days} to go)`;

  $("stat-best").textContent = Math.floor(s.best / DAY);
  $("stat-clean").textContent = s.clean;
  $("stat-urges").textContent = s.resisted;
  $("stat-relapses").textContent = s.relapses;

  renderCalendar(s.relapseDays);
  renderSettings();
}

function renderCalendar(relapseDays) {
  const weeks = 18;
  const today = new Date();
  today.setHours(12, 0, 0, 0);
  const startDay = new Date(snap.journey_start * 1000);
  startDay.setHours(0, 0, 0, 0);
  const urgeDays = new Set(snap.urges.filter((u) => u.note === "resisted").map((u) => dayKey(u.ts)));

  // Grid starts on the Saturday `weeks` weeks ago so columns are whole weeks.
  const first = new Date(today);
  first.setDate(first.getDate() - (weeks - 1) * 7 - ((today.getDay() + 1) % 7));
  const cal = $("cal");
  cal.replaceChildren();
  for (let i = 0; i < weeks * 7; i++) {
    const d = new Date(first);
    d.setDate(first.getDate() + i);
    const cell = document.createElement("i");
    cell.className = "cell";
    const key = d.toDateString();
    if (d > today) cell.classList.add("future");
    else if (relapseDays.has(key)) cell.classList.add("relapse");
    else if (d > startDay) cell.classList.add("clean");
    if (urgeDays.has(key)) cell.classList.add("urge");
    if (key === today.toDateString()) cell.classList.add("today");
    cell.title = d.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
    cal.append(cell);
  }
}

function renderSettings() {
  const st = snap.settings;
  $("set-reminders").checked = st.reminders;
  $("set-interval").value = String(st.interval_min);
  $("set-quiet-start").value = String(st.quiet_start);
  $("set-quiet-end").value = String(st.quiet_end);
  $("set-autostart").checked = snap.autostart;
}

// Shows a source like "سورة الزمر 39:53"; the trailing reference (or hadith
// number) is isolated left-to-right so its digits keep their order in Arabic text.
function setSource(el, source = "") {
  const i = source.lastIndexOf(" ");
  const ref = document.createElement("bdi");
  ref.dir = "ltr";
  ref.textContent = source.slice(i + 1);
  el.replaceChildren(source.slice(0, i + 1), ref);
}

async function showReading(kind) {
  const item = await invoke("pick_content", { kind });
  $("reading-text").textContent = item.text;
  $("reading-text").classList.toggle("quran", item.kind === "quran");
  setSource($("reading-source"), item.source);
}

async function refreshBlocker() {
  const pill = $("blocker-pill");
  const body = $("blocker-body");
  try {
    const s = await invoke("blocker_status");
    const paused = s.paused_until ? Math.ceil((s.paused_until - Date.now() / 1000) / 60) : 0;
    const state = s.active ? "ON" : s.disabled ? "DISABLED" : paused ? `PAUSED · ${paused} min` : "OFF";
    pill.textContent = `Blocker ${state}`;
    pill.className = `pill ${s.active ? "on" : "off"}`;
    body.innerHTML = "";
    const dl = document.createElement("dl");
    const rows = [
      ["Sites blocked", s.blocked_count.toLocaleString()],
      ["Friend password", s.password_set ? "set ✓" : "NOT SET"],
      ["Tamper attempts", String(s.tamper_count)],
      ["Lists updated", s.last_list_update ? new Date(s.last_list_update * 1000).toLocaleDateString() : "never"],
    ];
    for (const [k, v] of rows) {
      const dt = document.createElement("dt");
      const dd = document.createElement("dd");
      dt.textContent = k;
      dd.textContent = v;
      if (v === "NOT SET") dd.className = "warn";
      dl.append(dt, dd);
    }
    body.append(dl);
    $("cat-manga").checked = s.categories.includes("manga-anime");
    if (!s.password_set) {
      const p = document.createElement("p");
      p.className = "small warn";
      p.textContent = "Ask a trusted friend to run `ghadd set-password` in a terminal and type a password you don't see.";
      body.append(p);
    }
  } catch (e) {
    pill.textContent = "Blocker not running";
    pill.className = "pill off";
    body.textContent = `Can't reach the blocker service: ${e}`;
  }
}

// ---------- SOS ----------

let sosTimer = null;
let breathTimer = null;
let dhikrList = [];
let dhikrIndex = 0;
let dhikrCount = 0;

async function openSOS() {
  $("sos").hidden = false;
  showSOSAyah();
  if (!dhikrList.length) dhikrList = await invoke("dhikr");
  dhikrIndex = 0;
  dhikrCount = 0;
  renderDhikr();

  let left = 10 * 60;
  $("sos-timer").textContent = "10:00";
  clearInterval(sosTimer);
  sosTimer = setInterval(() => {
    left = Math.max(0, left - 1);
    $("sos-timer").textContent = `${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")}`;
    if (!left) clearInterval(sosTimer);
  }, 1000);
  breathe();
}

function breathe() {
  const c = $("breath-circle");
  const step = (inhale) => {
    c.classList.toggle("in", inhale);
    c.classList.toggle("out", !inhale);
    $("breath-word").textContent = inhale ? "Breathe in" : "Breathe out";
    breathTimer = setTimeout(() => step(!inhale), inhale ? 4000 : 6000);
  };
  clearTimeout(breathTimer);
  step(true);
}

async function showSOSAyah() {
  const item = await invoke("pick_content", { kind: "sos" });
  $("sos-ayah").textContent = item.text;
  setSource($("sos-ayah-src"), item.source);
}

function renderDhikr() {
  $("dhikr-text").textContent = dhikrList[dhikrIndex] ?? "";
  $("dhikr-count").textContent = dhikrCount;
}

async function closeSOS(outcome) {
  $("sos").hidden = true;
  clearInterval(sosTimer);
  clearTimeout(breathTimer);
  await invoke("log_urge", { outcome });
  refresh();
}

// ---------- events ----------

function toLocalInput(d) {
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

async function saveSettings() {
  const settings = {
    reminders: $("set-reminders").checked,
    interval_min: Number($("set-interval").value),
    quiet_start: Number($("set-quiet-start").value),
    quiet_end: Number($("set-quiet-end").value),
  };
  await invoke("save_settings", { settings });
  snap.settings = settings;
}

function wire() {
  for (const id of ["set-quiet-start", "set-quiet-end"]) {
    for (let h = 0; h < 24; h++) $(id).add(new Option(`${String(h).padStart(2, "0")}:00`, String(h)));
  }

  $("onboard-now").onclick = async () => {
    await invoke("start_journey", { ts: Math.floor(Date.now() / 1000) });
    refresh();
  };
  $("onboard-save").onclick = async () => {
    const t = new Date($("onboard-date").value).getTime();
    if (Number.isNaN(t)) return;
    await invoke("start_journey", { ts: Math.floor(t / 1000) });
    refresh();
  };

  $("relapse-btn").onclick = async () => {
    const ayah = await invoke("ayah", { reference: "39:53" });
    $("relapse-ayah").textContent = ayah?.text ?? "";
    setSource($("relapse-ayah-src"), ayah?.source);
    $("relapse-note").value = "";
    $("relapse").hidden = false;
  };
  $("relapse-cancel").onclick = () => ($("relapse").hidden = true);
  $("relapse-confirm").onclick = async () => {
    await invoke("log_relapse", { note: $("relapse-note").value });
    $("relapse").hidden = true;
    refresh();
  };

  $("sos-btn").onclick = openSOS;
  $("sos-next").onclick = showSOSAyah;
  $("sos-done").onclick = () => closeSOS("resisted");
  $("sos-close").onclick = () => closeSOS("closed");
  $("dhikr-btn").onclick = () => {
    dhikrCount++;
    if (dhikrCount > 33) {
      dhikrCount = 1;
      dhikrIndex = (dhikrIndex + 1) % dhikrList.length;
    }
    renderDhikr();
  };

  for (const b of document.querySelectorAll(".seg button")) b.onclick = () => showReading(b.dataset.kind);

  $("add-form").onsubmit = async (e) => {
    e.preventDefault();
    const domain = $("add-domain").value.trim();
    if (!domain) return;
    try {
      $("add-msg").textContent = await invoke("blocker_add", { domain });
      $("add-domain").value = "";
      refreshBlocker();
    } catch (err) {
      $("add-msg").textContent = String(err);
    }
  };

  $("cat-manga").onchange = async (e) => {
    if (!e.target.checked) {
      // Weakening the block always goes through the friend password.
      e.target.checked = true;
      $("cat-msg").textContent = "Turning this off needs your friend's password: ghadd category off manga-anime";
      return;
    }
    try {
      $("cat-msg").textContent = await invoke("blocker_category_on", { name: "manga-anime" });
    } catch (err) {
      e.target.checked = false;
      $("cat-msg").textContent = String(err);
    }
  };

  for (const id of ["set-reminders", "set-interval", "set-quiet-start", "set-quiet-end"]) $(id).onchange = saveSettings;
  $("set-autostart").onchange = async (e) => {
    try {
      await invoke("set_autostart", { enabled: e.target.checked });
    } catch (err) {
      e.target.checked = !e.target.checked;
      alert(err);
    }
  };
  $("test-notif").onclick = () => invoke("test_notification");

  listen("open-sos", openSOS);
}

wire();
await refresh();
showReading("any");
refreshBlocker();
setInterval(() => snap?.journey_start != null && render(), 30_000);
setInterval(refreshBlocker, 60_000);
