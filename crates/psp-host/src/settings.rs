//! Persisted user settings.
//!
//! Deliberately independent of Tauri: the store is handed a directory rather than
//! asking a global for one, so the load/save/patch behaviour is testable without
//! a running app. `main.rs` supplies the real config directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// Folders scanned for games.
    #[serde(default)]
    pub rom_paths: Vec<PathBuf>,
    /// Folders scanned for photos, music and video.
    #[serde(default)]
    pub media_paths: Vec<PathBuf>,
    /// Explicit PPSSPP binary. `None` means "look in the usual places".
    #[serde(default)]
    pub ppsspp_path: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub fullscreen: bool,
    #[serde(default = "default_true")]
    pub sound_enabled: bool,
    /// Overrides the month-derived XMB theme.
    #[serde(default)]
    pub theme_override: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rom_paths: Vec::new(),
            media_paths: Vec::new(),
            ppsspp_path: None,
            fullscreen: true,
            sound_enabled: true,
            theme_override: None,
        }
    }
}

fn default_true() -> bool {
    true
}

/// A partial update from the UI. Every field is optional so the front-end can
/// send only what changed; `None` means "leave alone".
///
/// The nullable fields are doubly wrapped: the outer `Option` distinguishes
/// "absent from the patch" from the inner "explicitly set to null", which is how
/// the UI clears an override.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SettingsPatch {
    pub rom_paths: Option<Vec<PathBuf>>,
    pub media_paths: Option<Vec<PathBuf>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub ppsspp_path: Option<Option<PathBuf>>,
    pub fullscreen: Option<bool>,
    pub sound_enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub theme_override: Option<Option<String>>,
}

/// Lets a patch field distinguish "not provided" from "explicitly null".
///
/// `Option<Option<T>>` normally collapses during deserialisation. Wrapping the
/// inner result in `Some` keeps the distinction, which is how the UI expresses
/// clearing an override rather than leaving it alone.
fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl Settings {
    pub fn apply(&mut self, patch: SettingsPatch) {
        if let Some(paths) = patch.rom_paths {
            self.rom_paths = paths;
        }
        if let Some(paths) = patch.media_paths {
            self.media_paths = paths;
        }
        if let Some(path) = patch.ppsspp_path {
            self.ppsspp_path = path;
        }
        if let Some(value) = patch.fullscreen {
            self.fullscreen = value;
        }
        if let Some(value) = patch.sound_enabled {
            self.sound_enabled = value;
        }
        if let Some(value) = patch.theme_override {
            self.theme_override = value;
        }
    }

    /// Adds a ROM folder, ignoring duplicates so repeated picks don't stack up.
    pub fn add_rom_path(&mut self, path: PathBuf) -> bool {
        if self.rom_paths.contains(&path) {
            return false;
        }
        self.rom_paths.push(path);
        true
    }

    /// Adds a media folder, ignoring duplicates.
    pub fn add_media_path(&mut self, path: PathBuf) -> bool {
        if self.media_paths.contains(&path) {
            return false;
        }
        self.media_paths.push(path);
        true
    }
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    /// Settings live at `<config_dir>/settings.json`.
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join(FILE_NAME),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads settings, falling back to defaults.
    ///
    /// A missing file is normal on first run. A corrupt one also yields defaults
    /// rather than an error: refusing to start because a JSON file got truncated
    /// would leave the user with no way in.
    pub fn load(&self) -> Settings {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Settings::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self, settings: &Settings) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(settings)?;
        // Write to a sibling then rename, so a crash mid-write cannot leave a
        // half-written settings file behind.
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, &self.path)
    }

    pub fn update(&self, patch: SettingsPatch) -> std::io::Result<Settings> {
        let mut settings = self.load();
        settings.apply(patch);
        self.save(&settings)?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        (dir, store)
    }

    #[test]
    fn a_missing_file_yields_defaults() {
        let (_dir, store) = store();
        let settings = store.load();
        assert_eq!(settings, Settings::default());
        assert!(settings.fullscreen);
        assert!(settings.sound_enabled);
    }

    #[test]
    fn saves_and_reloads() {
        let (_dir, store) = store();
        let mut settings = Settings::default();
        settings.rom_paths.push(PathBuf::from("/games/psp"));
        settings.ppsspp_path = Some(PathBuf::from("/usr/bin/PPSSPPSDL"));
        settings.sound_enabled = false;
        store.save(&settings).unwrap();

        assert_eq!(store.load(), settings);
    }

    #[test]
    fn a_corrupt_file_yields_defaults_rather_than_failing() {
        // Refusing to launch over a truncated JSON file would strand the user.
        let (dir, store) = store();
        std::fs::write(dir.path().join("settings.json"), b"{ this is not json").unwrap();
        assert_eq!(store.load(), Settings::default());
    }

    #[test]
    fn a_patch_only_changes_the_fields_it_carries() {
        let (_dir, store) = store();
        let mut initial = Settings::default();
        initial.rom_paths.push(PathBuf::from("/games"));
        store.save(&initial).unwrap();

        let patch: SettingsPatch = serde_json::from_str(r#"{"sound_enabled": false}"#).unwrap();
        let updated = store.update(patch).unwrap();

        assert!(!updated.sound_enabled);
        assert_eq!(updated.rom_paths, vec![PathBuf::from("/games")]);
        assert!(updated.fullscreen, "untouched fields keep their value");
    }

    #[test]
    fn an_explicit_null_clears_an_override() {
        let mut settings = Settings {
            theme_override: Some("December".into()),
            ..Settings::default()
        };

        let patch: SettingsPatch = serde_json::from_str(r#"{"theme_override": null}"#).unwrap();
        settings.apply(patch);
        assert_eq!(settings.theme_override, None);
    }

    #[test]
    fn an_absent_field_does_not_clear_an_override() {
        // The distinction that makes Option<Option<_>> worth the trouble.
        let mut settings = Settings {
            theme_override: Some("December".into()),
            ..Settings::default()
        };

        let patch: SettingsPatch = serde_json::from_str(r#"{"fullscreen": false}"#).unwrap();
        settings.apply(patch);
        assert_eq!(settings.theme_override.as_deref(), Some("December"));
    }

    #[test]
    fn unknown_fields_in_a_stored_file_are_tolerated() {
        // Forward compatibility: an older build must not choke on a newer file.
        let (dir, store) = store();
        std::fs::write(
            dir.path().join("settings.json"),
            br#"{"rom_paths":["/a"],"future_option":42}"#,
        )
        .unwrap();
        assert_eq!(store.load().rom_paths, vec![PathBuf::from("/a")]);
    }

    #[test]
    fn adding_a_rom_path_ignores_duplicates() {
        let mut settings = Settings::default();
        assert!(settings.add_rom_path(PathBuf::from("/games")));
        assert!(!settings.add_rom_path(PathBuf::from("/games")));
        assert_eq!(settings.rom_paths.len(), 1);
    }

    #[test]
    fn saving_creates_the_config_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("deep").join("config");
        let store = Store::new(&nested);
        store.save(&Settings::default()).unwrap();
        assert!(store.path().exists());
    }

    #[test]
    fn saving_leaves_no_temp_file_behind() {
        let (dir, store) = store();
        store.save(&Settings::default()).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "found {leftovers:?}");
    }
}
