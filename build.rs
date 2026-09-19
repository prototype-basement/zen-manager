use slint_build::{CompilerConfiguration, DefaultTranslationContext};

fn main() {
    // Translations are compiled into the binary rather than loaded through
    // system gettext, so the app has no runtime dependency to install and
    // language switching works offline.
    //
    // No default context means .po files carry a bare msgid, which is far
    // friendlier for contributors translating by hand.
    let config = CompilerConfiguration::new()
        .with_bundled_translations("translations")
        .with_default_translation_context(DefaultTranslationContext::None);

    slint_build::compile_with_config("ui/app.slint", config).unwrap();
}
