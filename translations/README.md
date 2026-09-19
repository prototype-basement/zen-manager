# Translating ZEN Manager

Translations are plain gettext `.po` files. You don't need to know Rust, and you
don't need to build the app to contribute one — though building it is the only
way to see your work in place.

## Adding a language

1. Copy the template into a folder named after your language code:

   ```
   mkdir -p translations/<lang>/LC_MESSAGES
   cp translations/zenmanager.pot translations/<lang>/LC_MESSAGES/zenmanager.po
   ```

   Use the short locale code — `de` for German, `pt_BR` for Brazilian
   Portuguese. The folder name is what the app matches against.

2. Fill in each `msgstr`. Leave it empty and the original English shows through,
   so a partial translation is fine and will not break anything.

   ```po
   msgid "Drop music here"
   msgstr "Ovdje ispusti glazbu"
   ```

3. Set the `Plural-Forms` line in the header for your language. If your language
   needs something other than the two-form default, add it to `PLURALS` in
   `sync.py` too, so it survives the next sync.

4. Register the language so it appears in the picker, by adding an entry to
   `LANGUAGES` in [`src/settings.rs`](../src/settings.rs):

   ```rust
   Language { code: "de", name: "Deutsch" },
   ```

   Write the name in the language itself — `Deutsch`, not `German`.

5. Rebuild. Translations are compiled into the binary, so a running app will not
   pick up `.po` edits until you build again. Once built, switching language in
   Settings applies immediately, with no restart.

## Placeholders

`{}` is substituted with a number at runtime:

```po
msgid "Transfer {} files"
msgstr "Prenesi {} datoteka"
```

Keep the `{}`. You may move it to wherever your grammar needs it.

## After changing the UI

If you add or edit `@tr("…")` text in `ui/app.slint`, resync the catalogues:

```
python3 translations/sync.py
```

It rewrites the `.pot`, appends new strings to every `.po`, drops removed ones,
and **preserves existing translations**. It also prints what each language is
still missing. Run it before committing — a string that is never extracted is a
string nobody can translate.

Slint also ships an official extractor with fuller plural support:

```
cargo install slint-tr-extractor
```

## What is not translatable yet

Status messages generated in Rust — transfer progress, error text, the queue
summary — are still English. Slint's `@tr()` only covers `.slint` files, so
making those translatable means passing structured values to the UI and
formatting them there. Worth doing; not done yet.
