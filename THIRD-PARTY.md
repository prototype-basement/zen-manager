# Third-party software

ZEN Manager is MIT-0. These components are not, and keep their own licences.
The same list appears in the app's About screen.

| Component | Used for | Licence |
|---|---|---|
| [Slint](https://slint.dev) | the user interface | Slint Royalty-free Desktop 2.0 |
| [libmtp](https://libmtp.sourceforge.net) | talking to the device over USB | LGPL-2.1-or-later |
| [libmtp-rs](https://crates.io/crates/libmtp-rs) | Rust bindings for libmtp | MIT |
| [Lofty](https://crates.io/crates/lofty) | reading audio tags | MIT or Apache-2.0 |
| [rfd](https://crates.io/crates/rfd) | native file dialogs | MIT |
| [winit](https://crates.io/crates/winit) | windowing and file drops | Apache-2.0 |
| [anyhow](https://crates.io/crates/anyhow) | error handling | MIT or Apache-2.0 |
| [chrono](https://crates.io/crates/chrono) | dates and times | MIT or Apache-2.0 |
| [dirs](https://crates.io/crates/dirs) | finding the config directory | MIT or Apache-2.0 |
| [Noto Sans](https://notofonts.github.io) | bundled fonts | SIL Open Font License 1.1 |
| [Lucide](https://lucide.dev) | icons | ISC |
| [Rust](https://rust-lang.org) | the language and toolchain | MIT or Apache-2.0 |

Plus the transitive dependencies these pull in; `cargo tree` lists them in full.

[Pillow](https://python-pillow.org) (MIT-CMU) is used by
`packaging/make-icons.sh` to generate the icon set. It runs in a throwaway
virtualenv, is never linked into the app, and is not needed to build or package
— the icons it produced are committed.

## Obligations when distributing

Being MIT-0 does not remove the conditions these components attach.

- **Slint** (royalty-free licence, §2) requires attribution: the `AboutSlint`
  widget in an About screen reachable from the top-level menu, or the
  "Made with Slint" badge on the page the binaries are downloaded from. The app
  shows the widget on its About page — **removing it breaks the licence**, and
  the fallback is GPL-3.0.
- **libmtp** is LGPL. It is linked dynamically, which keeps ZEN Manager's own
  licence unconstrained, but users must remain able to relink against their own
  build of libmtp. Don't statically link it without re-reading the LGPL.
- **Noto Sans** stays under the OFL. Keep `fonts/OFL.txt` alongside the fonts.
  The OFL also forbids selling the fonts on their own and reserves the Noto
  name for unmodified versions.
- **Lucide** is ISC, which requires its copyright notice to be retained. It is
  in the header of `ui/icons.slint`.

## Not bundled

CJK fonts are deliberately not included; those scripts resolve through the
operating system's own fonts.

## Artwork

The app icon depicts a Creative Zen Micro and carries the CREATIVE wordmark.
ZEN Manager is not affiliated with or endorsed by Creative Technology Ltd.
