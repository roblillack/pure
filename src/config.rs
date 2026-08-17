//! User configuration, loaded from a TOML file.
//!
//! Settings live in `~/.config/pure/config.toml` on every platform (honoring
//! `XDG_CONFIG_HOME` when set). We deliberately use the XDG location even on
//! macOS and Windows rather than `~/Library/Application Support` / `%APPDATA%`:
//! this is a file the user hand-edits, and terminal tools overwhelmingly keep
//! such config under `~/.config`, where it's visible and consistent. The file
//! is optional: a missing or unreadable file, or any unknown/invalid keys, fall
//! back to the defaults, so Pure always starts.

use std::env;
use std::path::PathBuf;

use serde::Deserialize;

/// Editor settings. Every field has a default (see [`Config::default`]), and
/// `#[serde(default)]` lets a partial file override only the keys it names.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Whether the caret observes *affinity* at inline-style/link boundaries —
    /// an extra navigation stop that lets the caret sit just inside vs. just
    /// outside a run, controlling whether typed text joins the run. On by
    /// default. In a terminal the affinity *lean* can't be drawn (Pure drives
    /// the hardware caret), so the only visible effect is that one Left/Right
    /// press at such a boundary flips affinity in place rather than moving.
    pub caret_affinity: bool,

    /// Language tag of the dictionary used for spell checking (F7), e.g.
    /// `en_US` or `de_DE`. Pure looks for `<tag>.aff` / `<tag>.dic` in
    /// [`Config::dictionary_dir`] first, then falls back to the bundled en_US
    /// dictionary and finally to `$DICPATH` and the system dictionary
    /// directories.
    pub spell_language: String,

    /// Explicit path to a Hunspell `.dic` file, bypassing the dictionary
    /// search. The matching `.aff` file is expected beside it under the same
    /// stem (`/path/to/en_GB.dic` → `/path/to/en_GB.aff`).
    pub spell_dictionary: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            caret_affinity: true,
            spell_language: "en_US".to_string(),
            spell_dictionary: None,
        }
    }
}

impl Config {
    /// Load the config, falling back to defaults when the file is absent,
    /// unreadable, or malformed. Best-effort by design: a broken config must
    /// never stop the editor from opening.
    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// The config file's path, `~/.config/pure/config.toml`.
    pub fn path() -> Option<PathBuf> {
        Some(Self::dir()?.join("config.toml"))
    }

    /// Pure's configuration directory, `~/.config/pure`. Honors an absolute
    /// `XDG_CONFIG_HOME` (per the XDG spec, a relative value is ignored) and
    /// otherwise falls back to `~/.config` — on every platform, not just Linux,
    /// so macOS/Windows users get the same visible, hand-editable location.
    /// `None` when the home directory can't be determined (rare; then only
    /// defaults apply).
    pub fn dir() -> Option<PathBuf> {
        let base = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .or_else(|| dirs::home_dir().map(|home| home.join(".config")))?;
        Some(base.join("pure"))
    }

    /// Where the user's own Hunspell dictionaries live,
    /// `~/.config/pure/dictionaries`.
    pub fn dictionary_dir() -> Option<PathBuf> {
        Some(Self::dir()?.join("dictionaries"))
    }

    /// The personal word list that "Add to Dictionary" appends to,
    /// `~/.config/pure/dictionary.txt` — one word per line.
    pub fn personal_dictionary_path() -> Option<PathBuf> {
        Some(Self::dir()?.join("dictionary.txt"))
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::Config;

    #[test]
    fn caret_affinity_defaults_on() {
        assert!(Config::default().caret_affinity);
        // An empty file leaves every field at its default.
        let config: Config = toml::from_str("").unwrap();
        assert!(config.caret_affinity);
    }

    #[test]
    fn caret_affinity_can_be_disabled() {
        let config: Config = toml::from_str("caret_affinity = false").unwrap();
        assert!(!config.caret_affinity);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        // A typo shouldn't be silently ignored; `load()` then falls back to
        // defaults rather than applying a half-parsed file.
        assert!(toml::from_str::<Config>("carrot_affinity = false").is_err());
    }

    #[test]
    fn path_uses_the_xdg_style_location() {
        let path = Config::path().expect("home dir resolvable in tests");
        // `ends_with` compares path components, so this holds on Windows too.
        assert!(
            path.ends_with("pure/config.toml"),
            "unexpected file name: {}",
            path.display()
        );
        // With no XDG override the file lives under `~/.config` on every
        // platform — not macOS's Library or Windows's AppData. (If the test
        // environment sets XDG_CONFIG_HOME we only check the filename above,
        // rather than mutating process env from a test.)
        if env::var_os("XDG_CONFIG_HOME").is_none() {
            assert!(
                path.components().any(|c| c.as_os_str() == ".config"),
                "config should live under .config, got {}",
                path.display()
            );
        }
    }
}
