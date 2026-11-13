//! What the user chose, kept in a small `key=value` file.
//!
//! A file that is missing, unreadable or half written is never an error: the app starts
//! with the defaults and says nothing, because preferences are not worth a failed launch.

use std::path::{Path, PathBuf};

/// How many recent cases the welcome screen lists.
pub const MAX_RECENT: usize = 8;

/// The light/dark choice of the Preferences dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Follows the system.
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    fn key(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prefs {
    pub theme: Theme,
    /// The cases opened last, the newest first.
    pub recent: Vec<PathBuf>,
}

impl Prefs {
    pub fn parse(text: &str) -> Self {
        let mut prefs = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "theme" => {
                    if let Some(theme) = Theme::from_key(value.trim()) {
                        prefs.theme = theme;
                    }
                }
                "recent" if !value.is_empty() => {
                    prefs.remember(Path::new(value));
                }
                _ => {}
            }
        }
        // `remember` puts the newest first and the file lists the newest first.
        prefs.recent.reverse();
        prefs
    }

    pub fn to_text(&self) -> String {
        let mut text = format!("theme={}\n", self.theme.key());
        for path in &self.recent {
            // A path with a line break could not be read back as one line.
            if let Some(path) = path.to_str().filter(|p| !p.contains(['\n', '\r'])) {
                text.push_str("recent=");
                text.push_str(path);
                text.push('\n');
            }
        }
        text
    }

    /// Puts a case at the top of the recent list.
    pub fn remember(&mut self, case: &Path) {
        self.recent.retain(|p| p != case);
        self.recent.insert(0, case.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Writes the file through a temporary one, so a crash never leaves half of it.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("conf.tmp");
        std::fs::write(&tmp, self.to_text())?;
        std::fs::rename(&tmp, path)
    }
}

/// Where the file lives: the user's configuration folder of the platform.
pub fn default_file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home?.join("Library/Application Support")
    } else {
        match std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
            Some(dir) if dir.is_absolute() => dir,
            _ => home?.join(".config"),
        }
    };
    Some(base.join("Vortia").join("rust-app.conf"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_round_trip_and_keep_the_order_of_the_recent_cases() {
        let mut prefs = Prefs {
            theme: Theme::Dark,
            ..Prefs::default()
        };
        prefs.remember(Path::new("/cases/a"));
        prefs.remember(Path::new("/cases/b"));
        prefs.remember(Path::new("/cases/a"));
        assert_eq!(
            prefs.recent,
            [PathBuf::from("/cases/a"), PathBuf::from("/cases/b")]
        );
        assert_eq!(Prefs::parse(&prefs.to_text()), prefs);
    }

    #[test]
    fn the_list_of_recent_cases_is_bounded() {
        let mut prefs = Prefs::default();
        for n in 0..(MAX_RECENT + 5) {
            prefs.remember(Path::new(&format!("/cases/{n}")));
        }
        assert_eq!(prefs.recent.len(), MAX_RECENT);
        assert_eq!(Prefs::parse(&prefs.to_text()).recent.len(), MAX_RECENT);
    }

    #[test]
    fn nonsense_in_the_file_falls_back_to_the_defaults() {
        let prefs = Prefs::parse("theme=neon\nno equals sign\nrecent=\n=\u{0}\n");
        assert_eq!(prefs, Prefs::default());
    }

    #[test]
    fn a_missing_file_is_the_defaults_and_saving_creates_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("deep/er/rust-app.conf");
        assert_eq!(Prefs::load_from(&file), Prefs::default());
        let mut prefs = Prefs {
            theme: Theme::Light,
            ..Prefs::default()
        };
        prefs.remember(Path::new("/cases/x"));
        prefs.save_to(&file).unwrap();
        assert_eq!(Prefs::load_from(&file), prefs);
    }

    #[test]
    fn a_path_with_a_line_break_is_not_written() {
        let mut prefs = Prefs::default();
        prefs.remember(Path::new("/cases/odd\nname"));
        assert!(!prefs.to_text().contains("odd"));
    }
}
