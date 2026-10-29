//! Quran ayat, hadith, reminders and dhikr bundled from data/content.json.

use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Ayah {
    #[serde(rename = "ref")]
    reference: String,
    surah: String,
    text: String,
    sos: bool,
}

#[derive(Deserialize)]
struct Hadith {
    text: String,
    source: String,
}

#[derive(Deserialize)]
pub struct Content {
    quran: Vec<Ayah>,
    hadith: Vec<Hadith>,
    reminders: Vec<String>,
    pub dhikr: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct Item {
    /// "quran", "hadith" or "reminder".
    pub kind: &'static str,
    pub text: String,
    pub source: String,
}

impl Content {
    pub fn load() -> Self {
        serde_json::from_str(include_str!("../../../data/content.json")).expect("bundled content.json is valid")
    }

    pub fn ayah(&self, reference: &str) -> Option<Item> {
        self.quran.iter().find(|a| a.reference == reference).map(Self::ayah_item)
    }

    fn ayah_item(a: &Ayah) -> Item {
        Item { kind: "quran", text: a.text.clone(), source: format!("سورة {} {}", a.surah, a.reference) }
    }

    /// `kind` is "quran", "hadith", "reminder", "sos" (ayat chosen for urges) or anything else for a mix.
    pub fn pick(&self, kind: &str) -> Item {
        let kind = match kind {
            "quran" | "hadith" | "reminder" | "sos" => kind,
            _ => match fastrand::u8(0..10) {
                0..=4 => "quran",
                5..=7 => "hadith",
                _ => "reminder",
            },
        };
        match kind {
            "hadith" => {
                let h = &self.hadith[fastrand::usize(..self.hadith.len())];
                Item { kind: "hadith", text: h.text.clone(), source: h.source.clone() }
            }
            "reminder" => {
                let r = &self.reminders[fastrand::usize(..self.reminders.len())];
                Item { kind: "reminder", text: r.clone(), source: String::new() }
            }
            _ => {
                let pool: Vec<&Ayah> = self.quran.iter().filter(|a| kind != "sos" || a.sos).collect();
                Self::ayah_item(pool[fastrand::usize(..pool.len())])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_content_loads_and_picks() {
        let c = Content::load();
        for kind in ["quran", "hadith", "reminder", "sos", "any"] {
            assert!(!c.pick(kind).text.is_empty(), "{kind}");
        }
        assert!(c.quran.iter().any(|a| a.sos));
        assert!(c.ayah("39:53").is_some());
        assert!(!c.dhikr.is_empty());
    }
}
