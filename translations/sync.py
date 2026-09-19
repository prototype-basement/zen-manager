#!/usr/bin/env python3
"""Regenerate the .pot template and sync every .po against the UI.

Existing translations are preserved; new strings are appended with an empty
msgstr and removed strings are dropped. Run this after changing any @tr() text:

    python3 translations/sync.py

Slint ships an official extractor (`cargo install slint-tr-extractor`) which
handles edge cases this script does not, such as plural forms. This exists so
contributors can update the catalogues without installing anything.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
UI = os.path.join(os.path.dirname(ROOT), "ui", "app.slint")
CRATE = "zenmanager"

PLURALS = {
    "hr": "nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 "
          "&& (n%100<10 || n%100>=20) ? 1 : 2);",
}
DEFAULT_PLURAL = "nplurals=2; plural=(n != 1);"


def extract_msgids(path):
    with open(path, encoding="utf-8") as handle:
        source = handle.read()
    return sorted(set(re.findall(r'@tr\("((?:[^"\\]|\\.)*)"', source)))


def read_po(path):
    """Returns {msgid: msgstr} for an existing catalogue, ignoring the header."""
    if not os.path.exists(path):
        return {}

    entries, msgid = {}, None
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line.startswith('msgid "'):
                msgid = line[7:-1]
            elif line.startswith('msgstr "') and msgid is not None:
                if msgid:
                    entries[msgid] = line[8:-1]
                msgid = None
    return entries


def write_po(path, language, msgids, translations):
    plural = PLURALS.get(language, DEFAULT_PLURAL)
    lines = [
        'msgid ""',
        'msgstr ""',
        f'"Project-Id-Version: {CRATE}\\n"',
        '"Report-Msgid-Bugs-To: \\n"',
        '"MIME-Version: 1.0\\n"',
        '"Content-Type: text/plain; charset=UTF-8\\n"',
        '"Content-Transfer-Encoding: 8bit\\n"',
        f'"Language: {language}\\n"',
        f'"Plural-Forms: {plural}\\n"',
        "",
    ]
    for msgid in msgids:
        lines.append(f'msgid "{msgid}"')
        lines.append(f'msgstr "{translations.get(msgid, "")}"')
        lines.append("")

    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("\n".join(lines))


def main():
    msgids = extract_msgids(UI)
    if not msgids:
        sys.exit(f"No @tr() strings found in {UI}")

    write_po(os.path.join(ROOT, f"{CRATE}.pot"), "", msgids, {})

    languages = sorted(
        name
        for name in os.listdir(ROOT)
        if os.path.isdir(os.path.join(ROOT, name, "LC_MESSAGES"))
    )

    print(f"{len(msgids)} strings in the UI")
    for language in languages:
        path = os.path.join(ROOT, language, "LC_MESSAGES", f"{CRATE}.po")
        existing = read_po(path)
        write_po(path, language, msgids, existing)

        done = sum(1 for m in msgids if existing.get(m))
        missing = [m for m in msgids if not existing.get(m)]
        # English needs no translations: an empty msgstr falls back to the msgid,
        # which is already English.
        note = "" if language == "en" else f"  missing: {missing}" if missing else ""
        print(f"  {language}: {done}/{len(msgids)} translated{note}")


if __name__ == "__main__":
    main()
