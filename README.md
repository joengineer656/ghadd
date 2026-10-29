# Ghadd · غضّ البصر

**قُل لِّلْمُؤْمِنِينَ يَغُضُّوا مِنْ أَبْصَارِهِمْ وَيَحْفَظُوا فُرُوجَهُمْ ۚ ذَٰلِكَ أَزْكَىٰ لَهُمْ ۗ إِنَّ اللَّهَ خَبِيرٌ بِمَا يَصْنَعُونَ (30)**

**Guard your gaze, one day at a time.** An open-source app for Arch-based Linux (made on CachyOS) that helps Muslims quit pornography:

- 🛡️ **System-wide blocker.** About 116,000 adult sites are blocked in `/etc/hosts`, and a family DNS filter (Cloudflare 1.1.1.3) catches the rest.
- 🔒 **Friend-password lock.** Unblocking, pausing or disabling needs a password that a trusted friend sets.
- 🧱 **Bypass protection.** SafeSearch is forced, along with YouTube Restricted Mode. Browser DNS-over-HTTPS is turned off, and so are Brave's Tor windows. A watchdog repairs any change within 30 seconds and logs it.
- 📈 **Streak tracker.** Current and best streak, clean days, milestones and an 18-week calendar. A relapse resets the streak but never erases your history.
- 📖 **Reminders.** Notifications of Quran ayat and authentic hadith in Arabic, with references.
- 🆘 **SOS screen.** For when an urge hits: an ayah, a breathing timer, a dhikr counter and things to do right now.

Your journal stays on your computer. Ghadd has no accounts and no tracking.

## Install

```sh
git clone <this repo> ghadd && cd ghadd/packaging
makepkg -si
sudo systemctl enable --now ghaddd
ghadd set-password        # let your friend type it
```

Then open **Ghadd** from the app menu.

## Command line

| Command | Needs friend password |
| --- | --- |
| `ghadd status` | no |
| `ghadd add <site>` | no, blocking more is always allowed |
| `ghadd remove <site>` | yes |
| `ghadd pause <minutes>` | yes (up to 24 h, then it turns back on) |
| `ghadd disable` / `ghadd enable` | yes / no |
| `ghadd category on manga-anime` | no (blocks manga, webtoons, comics & anime) |
| `ghadd category off manga-anime` | yes |
| `ghadd tamper-log` | no |
| `ghadd update-lists` | no (also runs weekly) |

Settings live in `/etc/ghadd/config.toml`. All fields are optional:

```toml
youtube_restricted = true          # YouTube Restricted Mode
dns_servers = ["1.1.1.3", "1.0.0.3"]
manage_dns = true
manage_browsers = true
```

## An honest limitation

Anyone with `sudo` can eventually defeat any blocker on their own computer. Ghadd makes that slow, deliberate and visible (it logs every attempt), so a weak moment has time to pass. For extra strength, also set the family DNS on your home router, and let your friend keep the router password too.

## How it works

- `ghaddd` is a root systemd service. It owns a marked section of `/etc/hosts` and these files: `/etc/systemd/resolved.conf.d/ghadd.conf`, `/etc/NetworkManager/conf.d/ghadd.conf`, `/etc/resolv.conf`, and browser policies under `/etc/{firefox,brave,chromium,opt/chrome}/policies`. It locks them with the immutable flag and checks them every 30 seconds. Any file it replaced is backed up and restored when blocking is turned off or the package is removed.
- `ghadd` (CLI) and `ghadd-app` (Tauri desktop app) talk to it over `/run/ghadd.sock`.
- The friend password is stored only as an argon2 hash, in `/etc/ghadd/lock`.

## Content sources

- Quran: Uthmani text from [quran-json](https://github.com/risan/quran-json) (The Noble Qur'an Encyclopedia).
- Hadith: Arabic editions from [hadith-api](https://github.com/fawazahmed0/hadith-api). The numbers follow the standard Arabic editions, and Sunan grades are Al-Albani's.
- `data/build_content.py` regenerates `data/content.json`. Texts are copied verbatim, never retyped.
- Arabic fonts: [Amiri and Amiri Quran](https://github.com/aliftype/amiri) (SIL OFL 1.1), bundled so harakat render correctly on every system.
- Manga/anime category: hand-curated in `data/blocklists/manga-anime.txt` (no maintained public list exists). Pull requests welcome.
- Blocklists: [StevenBlack/hosts](https://github.com/StevenBlack/hosts) (porn-only) and [Anti-Porn HOSTS File](https://github.com/4skinSkywalker/Anti-Porn-HOSTS-File).

## Documentation

Open `docs/index.html` for the illustrated user guide. Regenerate its screenshots with `python3 docs/shots/render.py`.

## Build from source

```sh
cargo build --release            # ghaddd, ghadd, ghadd-app
cargo test
cd app && npm install && npx tauri dev   # run the app with live reload
```

---

## بالعربية

**غضّ** تطبيق مفتوح المصدر لأنظمة لينكس المبنية على آرتش، يساعد المسلم على ترك المواد الإباحية:

- **حجب شامل** لنحو ١١٦ ألف موقع، مع DNS عائلي يحجب ما لم يرد في القوائم.
- **قفل بكلمة سرّ يضعها صديق تثق به.** لا يمكن إيقاف الحجب أو تعليقه أو رفع الحجب عن موقع إلا بها.
- **حماية من التحايل:** فرض البحث الآمن ووضع يوتيوب المقيّد، وتعطيل DNS المشفّر في المتصفح ونوافذ Tor في Brave، وإصلاح أي تعديل خلال ٣٠ ثانية وتسجيله.
- **عدّاد الأيام:** السلسلة الحالية وأفضل سلسلة وتقويم. الانتكاسة تعيد العدّ لكنها لا تمحو تاريخك.
- **تذكير بآيات من القرآن وأحاديث صحيحة** مع ذكر مصادرها.
- **زر الطوارئ (SOS):** آية، وتمرين تنفّس، وعدّاد أذكار، وخطوات عملية تقوم بها فورًا.

بياناتك تبقى على جهازك فقط.

**التثبيت:** نفّذ الأوامر الموجودة في قسم Install أعلاه، ثم افتح «Ghadd» من قائمة التطبيقات.

**ملاحظة صادقة:** من يملك صلاحيات `sudo` يستطيع في النهاية تجاوز أي برنامج حجب على جهازه. غاية هذا التطبيق أن يجعل التجاوز بطيئًا ومقصودًا ومسجّلًا، حتى تمرّ لحظة الضعف. ولمزيد من القوة، اضبط الـDNS العائلي على الراوتر أيضًا، واترك كلمة سرّه مع صديقك.

نسأل الله أن يعين كل من يستخدمه ويثبّته.
