"""Builds ghadd's data/content.json from the downloaded Quran and hadith datasets.

Every Quran and hadith text is copied verbatim from the source; phrases below
are only used to locate the matn inside the full narration.
"""
import json
import re
import sys
from pathlib import Path

SRC = Path(sys.argv[1])
OUT = Path(sys.argv[2])

# (surah, first ayah, last ayah, show in SOS screen)
QURAN = [
    (24, 30, 30, True), (17, 32, 32, True), (96, 14, 14, True), (79, 40, 41, True),
    (7, 201, 201, True), (39, 53, 53, True), (12, 33, 33, True), (40, 19, 19, True),
    (17, 36, 36, True), (29, 45, 45, True), (13, 28, 28, True), (50, 16, 16, True),
    (23, 1, 1, False), (23, 5, 7, False), (2, 45, 45, False), (2, 153, 153, False),
    (2, 186, 186, False), (3, 133, 133, False), (3, 135, 135, False),
    (3, 200, 200, False), (4, 27, 28, False), (4, 110, 110, False), (6, 120, 120, False),
    (11, 114, 114, False), (11, 115, 115, False), (12, 53, 53, False), (15, 49, 49, False),
    (16, 128, 128, False), (20, 82, 82, False), (24, 21, 21, False), (25, 70, 70, False),
    (29, 69, 69, False), (33, 41, 42, False), (47, 7, 7, False), (66, 8, 8, False),
    (89, 27, 28, False), (94, 5, 6, False),
]

# (edition, book name, start phrase, end phrase) — phrases are matched after normalization.
HADITH = [
    ("ara-bukhari", "صحيح البخاري", "يا معشر الشباب", "فانه له وجاء"),
    ("ara-bukhari", "صحيح البخاري", "من يضمن لي ما بين لحييه", "اضمن له الجنه"),
    ("ara-bukhari", "صحيح البخاري", "الله افرح بتوبه عبده", "فلاه"),
    ("ara-bukhari", "صحيح البخاري", "ان الله كتب علي ابن ادم حظه من الزنا", "ويكذبه"),
    ("ara-bukhari", "صحيح البخاري", "ليس الشديد بالصرعه", "عند الغضب"),
    ("ara-muslim", "صحيح مسلم", "ان الله عز وجل يبسط يده بالليل", "من مغربها"),
    ("ara-muslim", "صحيح مسلم", "حفت الجنه بالمكاره", "بالشهوات"),
    ("ara-muslim", "صحيح مسلم", "اللهم اني اسالك الهدي والتقي", "والغني"),
    ("ara-muslim", "صحيح مسلم", "ان الله لا ينظر الي صوركم", "واعمالكم"),
    ("ara-tirmidhi", "سنن الترمذي", "كل ابن ادم خطاء", "التوابون"),
    ("ara-tirmidhi", "سنن الترمذي", "يا مقلب القلوب ثبت قلبي علي دينك", "علي دينك"),
    ("ara-abudawud", "سنن أبي داود", "لا تتبع النظره النظره", "الاخره"),
    ("ara-nawawi", "الأربعون النووية", "اتق الله حيثما كنت", "بخلق حسن"),
    ("ara-nawawi", "الأربعون النووية", "احفظ الله يحفظك", "تجاهك"),
    ("ara-nawawi", "الأربعون النووية", "يا ابن ادم انك ما دعوتني ورجوتني", "مغفره"),
    ("ara-nawawi", "الأربعون النووية", "انما الاعمال بالنيات", "ما نوي"),
]

# Short reminders written for the app (not attributed to anyone).
REMINDERS = [
    "الرغبة موجة، تعلو ثم تنكسر. اصبر عشر دقائق وستمرّ بإذن الله.",
    "قم الآن، توضّأ، وصلِّ ركعتين.",
    "أغلق الشاشة واخرج قليلًا، أو اتصل بصديق.",
    "كل يوم تنتصر فيه على نفسك خطوة نحو قلبٍ أنقى.",
    "إن سقطت فقم، وجدّد التوبة؛ فباب الله مفتوح.",
    "غضُّ بصرك اليوم يحفظ قلبك غدًا.",
    "الفراغ والوحدة والسهر مداخل للشيطان، فاملأ وقتك بما ينفعك.",
    "تذكّر لماذا بدأت.",
    "نَم مبكرًا؛ فالسهر يُضعف الإرادة.",
    "استعذ بالله من الشيطان الرجيم، وغيّر مكانك الآن.",
    "لست وحدك في هذه المعركة، والله يحب من يجاهد نفسه.",
    "مارس رياضة اليوم؛ الجسد القوي يعين على النفس القوية.",
]

DHIKR = [
    "أعوذ بالله من الشيطان الرجيم",
    "أستغفر الله",
    "سبحان الله",
    "الحمد لله",
    "لا إله إلا الله",
    "الله أكبر",
    "لا حول ولا قوة إلا بالله",
    "حسبي الله ونعم الوكيل",
]

ARABIC_DIGITS = str.maketrans("0123456789", "٠١٢٣٤٥٦٧٨٩")

# Narrations the dataset leaves unnumbered; placed between its 2758 and 2760.
NUMBER_FIXES = {("ara-muslim", 6989): "2759", ("ara-muslim", 6990): "2759"}

GRADES = {"Sahih": "صححه الألباني", "Hasan": "حسّنه الألباني"}

DIACRITICS = re.compile("[ؐ-ًؚ-ٰٟۖ-ۭـ]")
PUNCTUATION = re.compile('[!"،,.:؛؟?]')
LETTERS = str.maketrans({"أ": "ا", "إ": "ا", "آ": "ا", "ٱ": "ا", "ى": "ي", "ة": "ه", "ؤ": "و", "ئ": "ي"})


def normalize_with_map(text):
    """Strips diacritics and unifies letter forms, keeping a map back to `text` offsets."""
    chars, index = [], []
    for i, ch in enumerate(text):
        if DIACRITICS.match(ch) or PUNCTUATION.match(ch):
            continue
        chars.append(ch.translate(LETTERS))
        index.append(i)
    return "".join(chars), index


def norm(phrase):
    return normalize_with_map(phrase)[0]


def extract(text, start, end):
    plain, index = normalize_with_map(text)
    s = plain.find(norm(start))
    if s < 0:
        return None
    e = plain.find(norm(end), s)
    if e < 0:
        return None
    e += len(norm(end)) - 1
    # Extend to include trailing diacritics of the last letter.
    stop = index[e] + 1
    while stop < len(text) and DIACRITICS.match(text[stop]):
        stop += 1
    return text[index[s]:stop]


def main():
    quran = json.loads((SRC / "quran.json").read_text())
    content = {"quran": [], "hadith": [], "reminders": REMINDERS, "dhikr": DHIKR}
    for surah, first, last, sos in QURAN:
        s = quran[surah - 1]
        # ۞ (rub' al-hizb) marks a mushaf section, it isn't part of the ayah.
        verses = {v["id"]: v["text"].replace("۞", "").strip() for v in s["verses"]}
        text = " ".join(f"{verses[a]} ﴿{str(a).translate(ARABIC_DIGITS)}﴾" for a in range(first, last + 1))
        ref = f"{surah}:{first}" + (f"-{last}" if last != first else "")
        content["quran"].append({"ref": ref, "surah": s["name"], "text": text, "sos": sos})

    editions = {}
    for edition, book, start, end in HADITH:
        if edition not in editions:
            editions[edition] = json.loads((SRC / f"{edition}.json").read_text())["hadiths"]
        # Only entries with a standard (Arabic edition) number, so users can look them up.
        def standard_number(h):
            return NUMBER_FIXES.get((edition, h["hadithnumber"])) or h.get("arabicnumber")

        found = [(h, extract(h["text"], start, end)) for h in editions[edition] if standard_number(h)]
        found = [(h, t) for h, t in found if t]
        if not found:
            sys.exit(f"NOT FOUND: {edition} {start}")
        h, matn = min(found, key=lambda ht: len(ht[1]))  # shortest match = tightest wording
        number = str(standard_number(h)).split(".")[0]
        source = f"{book} {number}"
        grade = next((GRADES.get(g["grade"]) for g in h.get("grades", []) if g["name"] == "Al-Albani"), None)
        if grade:
            source += f" ({grade})"
        content["hadith"].append({"text": matn, "source": source})

    OUT.write_text(json.dumps(content, ensure_ascii=False, indent=1) + "\n")
    print(f"{len(content['quran'])} ayat, {len(content['hadith'])} hadith")


main()
