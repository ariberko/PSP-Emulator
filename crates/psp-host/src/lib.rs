//! Host-side logic for the desktop app, deliberately free of Tauri.
//!
//! The Tauri crate needs system webview libraries to build, which makes anything
//! living inside it awkward to test and impossible to check on a machine without
//! them. So everything with real behaviour — settings persistence, locating
//! PPSSPP, launching a game, scanning the library — lives here and is covered by
//! ordinary `cargo test`. The Tauri crate is left as a thin layer that maps IPC
//! commands onto these functions.
//!
//! ```no_run
//! use psp_host::{emulator, settings::Store};
//! use std::path::Path;
//!
//! let store = Store::new(Path::new("/home/me/.config/psp-emulator"));
//! let settings = store.load();
//! let scan = psp_host::scan(&settings);
//! if let Some(first) = scan.games.first() {
//!     emulator::launch(settings.ppsspp_path.as_deref(), &first.path, settings.fullscreen)?;
//! }
//! # Ok::<(), emulator::LaunchError>(())
//! ```

pub mod bundled_roms;
pub mod emulator;
pub mod ppsspp_config;
pub mod save_states;
pub mod settings;

pub use bundled_roms::{BundledRom, InstallReport};
pub use emulator::{launch, resolve, EmulatorStatus, LaunchError};
pub use ppsspp_config::{load_pad_profile, PadProfile};
pub use save_states::{scan_save_states, SaveState, SaveStateScan};
pub use settings::{Settings, SettingsPatch, Store};

use psp_metadata::{LibraryScan, MediaScan};

/// Scans every configured ROM folder.
pub fn scan(settings: &Settings) -> LibraryScan {
    psp_metadata::scan_library(&settings.rom_paths)
}

/// Scans every configured media folder for photos, music and video.
pub fn scan_media(settings: &Settings) -> MediaScan {
    psp_metadata::scan_media(&settings.media_paths)
}

/// Installs the bundled games and adds their folder to the library.
///
/// Two steps that must not come apart: copying the games without registering the
/// folder leaves the user staring at the same empty list, which reads as the
/// button having done nothing.
///
/// The folder is registered whenever it ends up holding games — including when
/// they were all already there from a previous run — but not when the build ships
/// no games at all, since pointing the scanner at a folder that does not exist
/// would only produce a "missing folder" warning.
pub fn install_bundled_roms(
    store: &Store,
    source: &std::path::Path,
    target: &std::path::Path,
) -> std::io::Result<(Settings, InstallReport)> {
    let report = bundled_roms::install(source, target)?;
    let mut settings = store.load();
    if report.is_populated() && settings.add_rom_path(target.to_path_buf()) {
        store.save(&settings)?;
    }
    Ok((settings, report))
}

/// Version string reported to the UI by the "System Information" item.
pub fn host_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Finds PPSSPP's save states, for cloud sync.
pub fn save_states(settings: &Settings) -> SaveStateScan {
    let status = emulator::resolve(settings.ppsspp_path.as_deref());
    save_states::scan_save_states(status.path.as_deref())
}

/// Reads the controller mapping PPSSPP already has, if any.
///
/// Resolves the emulator first so a portable Windows install — which keeps its
/// memory stick beside the executable — is found as well as the standard
/// home-directory locations.
pub fn pad_profile(settings: &Settings) -> PadProfile {
    let status = emulator::resolve(settings.ppsspp_path.as_deref());
    ppsspp_config::load_pad_profile(status.path.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanning_with_no_configured_folders_is_empty_and_quiet() {
        let scan = scan(&Settings::default());
        assert!(scan.games.is_empty());
        assert!(scan.problems.is_empty());
        assert!(scan.missing_roots.is_empty());
    }

    #[test]
    fn scanning_reports_a_configured_folder_that_is_gone() {
        let settings = Settings {
            rom_paths: vec!["/definitely/not/here".into()],
            ..Settings::default()
        };
        assert_eq!(scan(&settings).missing_roots.len(), 1);
    }

    #[test]
    fn host_version_is_not_empty() {
        assert!(!host_version().is_empty());
    }

    /// Source folder holding one real, parseable homebrew package.
    fn bundle() -> tempfile::TempDir {
        use psp_metadata::testkit::{PbpBuilder, SfoBuilder};
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Bundled.pbp"),
            PbpBuilder::new()
                .param_sfo(
                    SfoBuilder::new()
                        .text("CATEGORY", "MG")
                        .text("TITLE", "Bundled Homebrew")
                        .build(),
                )
                .data_psp(b"placeholder".to_vec())
                .build(),
        )
        .unwrap();
        dir
    }

    #[test]
    fn installing_bundled_roms_registers_the_folder_and_the_games_then_scan() {
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path());
        let source = bundle();
        let target = tempfile::tempdir().unwrap();

        let (settings, report) = install_bundled_roms(&store, source.path(), target.path()).unwrap();

        assert_eq!(report.installed, ["Bundled.pbp"]);
        assert!(settings.rom_paths.contains(&target.path().to_path_buf()));
        // Persisted, not just returned — the next launch must still see it.
        assert_eq!(store.load().rom_paths, settings.rom_paths);

        let scan = scan(&settings);
        assert_eq!(scan.games.len(), 1);
        assert_eq!(scan.games[0].title, "Bundled Homebrew");
    }

    #[test]
    fn installing_twice_does_not_add_the_folder_twice() {
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path());
        let source = bundle();
        let target = tempfile::tempdir().unwrap();

        install_bundled_roms(&store, source.path(), target.path()).unwrap();
        let (settings, report) = install_bundled_roms(&store, source.path(), target.path()).unwrap();

        assert!(report.installed.is_empty());
        assert_eq!(report.already_present, ["Bundled.pbp"]);
        assert_eq!(settings.rom_paths.len(), 1);
    }

    #[test]
    fn a_build_with_no_bundled_games_leaves_the_library_untouched() {
        // Registering a folder that was never created would surface as a spurious
        // "that folder is missing" warning in the UI.
        let config = tempfile::tempdir().unwrap();
        let store = Store::new(config.path());
        let empty = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap().path().join("Bundled Games");

        let (settings, report) = install_bundled_roms(&store, empty.path(), &target).unwrap();

        assert!(!report.is_populated());
        assert!(settings.rom_paths.is_empty());
        assert!(!target.exists());
    }
}
