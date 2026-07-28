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

pub mod emulator;
pub mod settings;

pub use emulator::{launch, resolve, EmulatorStatus, LaunchError};
pub use settings::{Settings, SettingsPatch, Store};

use psp_metadata::LibraryScan;

/// Scans every configured ROM folder.
pub fn scan(settings: &Settings) -> LibraryScan {
    psp_metadata::scan_library(&settings.rom_paths)
}

/// Version string reported to the UI by the "System Information" item.
pub fn host_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
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
}
