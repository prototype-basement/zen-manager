use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Theme {
    #[default]
    Auto,
    Light,
    Dark,
}

impl Theme {
    pub fn from_code(code: i32) -> Self {
        match code {
            1 => Theme::Light,
            2 => Theme::Dark,
            _ => Theme::Auto,
        }
    }

    pub fn code(self) -> i32 {
        match self {
            Theme::Auto => 0,
            Theme::Light => 1,
            Theme::Dark => 2,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Theme::Auto => "auto",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            _ => Theme::Auto,
        }
    }
}

/// The languages the app ships translations for. `code` must match the
/// directory name under `translations/`, and an empty code means "follow the
/// system", which is what Slint does when no translation is selected.
pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
}

pub const LANGUAGES: &[Language] = &[
    Language { code: "", name: "System default" },
    Language { code: "en", name: "English" },
    Language { code: "hr", name: "Hrvatski" },
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub theme: Theme,
    /// Empty means follow the system locale.
    pub language: String,
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("zenmanager").join("settings.conf"))
}

impl Settings {
    pub fn load() -> Self {
        config_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Best effort: a config that cannot be written is not worth interrupting
    /// the person over, they just get defaults next launch.
    pub fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, self.serialise());
    }

    fn serialise(&self) -> String {
        format!("theme={}\nlanguage={}\n", self.theme.as_str(), self.language)
    }

    fn parse(text: &str) -> Self {
        let mut settings = Settings::default();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "theme" => settings.theme = Theme::parse(value.trim()),
                "language" => settings.language = value.trim().to_string(),
                _ => {}
            }
        }

        settings
    }

    pub fn language_index(&self) -> i32 {
        LANGUAGES
            .iter()
            .position(|l| l.code == self.language)
            .unwrap_or(0) as i32
    }

    pub fn language_from_index(index: i32) -> String {
        LANGUAGES
            .get(index.max(0) as usize)
            .map(|l| l.code.to_string())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_auto_theme_and_system_language() {
        let settings = Settings::default();
        assert_eq!(settings.theme, Theme::Auto);
        assert_eq!(settings.language, "");
        assert_eq!(settings.language_index(), 0);
    }

    #[test]
    fn settings_survive_a_save_and_load_round_trip() {
        let original = Settings {
            theme: Theme::Dark,
            language: "hr".into(),
        };

        let restored = Settings::parse(&original.serialise());

        assert_eq!(restored, original);
    }

    #[test]
    fn an_unreadable_config_falls_back_to_defaults_rather_than_failing() {
        assert_eq!(Settings::parse("total nonsense"), Settings::default());
        assert_eq!(Settings::parse(""), Settings::default());
    }

    #[test]
    fn unknown_keys_and_comments_are_ignored() {
        let settings = Settings::parse("# a comment\nfavourite=pizza\ntheme=light\n");

        assert_eq!(settings.theme, Theme::Light);
    }

    #[test]
    fn an_unknown_theme_value_falls_back_to_auto() {
        assert_eq!(Settings::parse("theme=chartreuse").theme, Theme::Auto);
    }

    #[test]
    fn theme_codes_round_trip_through_the_ui_representation() {
        for theme in [Theme::Auto, Theme::Light, Theme::Dark] {
            assert_eq!(Theme::from_code(theme.code()), theme);
        }
        // Anything the UI could not produce is treated as auto.
        assert_eq!(Theme::from_code(99), Theme::Auto);
    }

    #[test]
    fn language_index_maps_both_ways() {
        let settings = Settings {
            language: "hr".into(),
            ..Default::default()
        };
        let index = settings.language_index();

        assert_eq!(Settings::language_from_index(index), "hr");
        assert_eq!(Settings::language_from_index(0), "");
        // Out of range cannot panic; the picker is driven by this same list.
        assert_eq!(Settings::language_from_index(999), "");
    }

    #[test]
    fn a_language_no_longer_shipped_falls_back_to_system_default() {
        let settings = Settings {
            language: "tlh".into(),
            ..Default::default()
        };

        assert_eq!(settings.language_index(), 0);
    }
}
