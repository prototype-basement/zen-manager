use slint::fontique_011::fontique;

/// Noto Sans already covers Latin (including Polish, Croatian, Czech, Turkish
/// and Vietnamese), Greek and Cyrillic in one family, so those three scripts
/// need no separate fallback. Arabic and Hebrew ship as their own families.
///
/// CJK is deliberately not bundled: those fonts run to tens of megabytes each
/// and every desktop OS already provides them, so they resolve through the
/// system fallback instead.
const LATIN: &[&[u8]] = &[
    include_bytes!("../fonts/NotoSans-Regular.ttf"),
    include_bytes!("../fonts/NotoSans-Bold.ttf"),
];

const ARABIC: &[&[u8]] = &[
    include_bytes!("../fonts/NotoSansArabic-Regular.ttf"),
    include_bytes!("../fonts/NotoSansArabic-Bold.ttf"),
];

const HEBREW: &[&[u8]] = &[
    include_bytes!("../fonts/NotoSansHebrew-Regular.ttf"),
    include_bytes!("../fonts/NotoSansHebrew-Bold.ttf"),
];

fn register(collection: &mut fontique::Collection, fonts: &[&'static [u8]], scripts: &[&str]) {
    for data in fonts {
        let blob = fontique::Blob::new(std::sync::Arc::new(data.to_vec()));
        let registered = collection.register_fonts(blob, None);

        for script in scripts {
            collection.append_fallbacks(
                fontique::FallbackKey::new(fontique::Script::from_str_unchecked(script), None),
                registered.iter().map(|font| font.0),
            );
        }
    }
}

/// Registers the bundled fonts process-wide. Must run before the first window
/// is shown, or early text will be laid out with the system font instead.
pub fn install() {
    let mut collection = slint::fontique_011::shared_collection();

    register(&mut collection, LATIN, &["Latn", "Grek", "Cyrl"]);
    register(&mut collection, ARABIC, &["Arab"]);
    register(&mut collection, HEBREW, &["Hebr"]);
}
