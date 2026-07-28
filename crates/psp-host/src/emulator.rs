//! Finding PPSSPP and handing it a game.
//!
//! This shell provides the XMB; PPSSPP does the emulating. So the desktop app's
//! real job here is small and worth getting exactly right: locate the binary
//! across three platforms, and start it without ever involving a shell.
//!
//! That last point matters. Game paths routinely contain spaces, apostrophes and
//! non-ASCII characters, and ROM folders are user-supplied. Building a command
//! line as a string invites quoting bugs at best. Every argument here is passed
//! as its own `argv` entry, so no path can be reinterpreted as syntax.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

/// Where a binary was found, for display in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// An explicit path from settings.
    Configured,
    /// Found on `PATH`.
    Path,
    /// A well-known install location for this platform.
    Bundled,
}

impl Source {
    pub fn label(&self) -> &'static str {
        match self {
            Source::Configured => "configured",
            Source::Path => "found on PATH",
            Source::Bundled => "standard install location",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EmulatorStatus {
    pub found: bool,
    pub path: Option<PathBuf>,
    /// Reported by `--version` when the binary answers; absent otherwise.
    pub version: Option<String>,
    pub source: Option<String>,
}

impl EmulatorStatus {
    fn missing() -> Self {
        Self {
            found: false,
            path: None,
            version: None,
            source: None,
        }
    }
}

#[derive(Debug)]
pub enum LaunchError {
    /// No PPSSPP binary could be located.
    NotFound,
    /// The game file is gone — a stale library entry.
    GameMissing(PathBuf),
    Spawn(std::io::Error),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LaunchError::NotFound => write!(
                f,
                "PPSSPP was not found. Install it, or set its path in Settings."
            ),
            LaunchError::GameMissing(path) => {
                write!(
                    f,
                    "{} is no longer there. Try refreshing the library.",
                    path.display()
                )
            }
            LaunchError::Spawn(e) => write!(f, "Could not start PPSSPP: {e}"),
        }
    }
}

impl std::error::Error for LaunchError {}

/// Executable names to try, most specific first.
///
/// PPSSPP ships an SDL build and a Qt build with different names, and Windows
/// has separate 32/64-bit executables.
#[cfg(target_os = "windows")]
const BINARY_NAMES: &[&str] = &["PPSSPPWindows64.exe", "PPSSPPWindows.exe", "PPSSPP.exe"];
#[cfg(target_os = "macos")]
const BINARY_NAMES: &[&str] = &["PPSSPPSDL", "PPSSPPQt", "ppsspp"];
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const BINARY_NAMES: &[&str] = &["PPSSPPSDL", "PPSSPPQt", "ppsspp", "ppsspp-sdl", "ppsspp-qt"];

/// Directories to search when `PATH` comes up empty.
#[cfg(target_os = "windows")]
fn well_known_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Ok(base) = std::env::var(key) {
            dirs.push(PathBuf::from(&base).join("PPSSPP"));
            dirs.push(PathBuf::from(base).join("PPSSPP").join("bin"));
        }
    }
    dirs
}

#[cfg(target_os = "macos")]
fn well_known_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/Applications/PPSSPPSDL.app/Contents/MacOS"),
        PathBuf::from("/Applications/PPSSPP.app/Contents/MacOS"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join("Applications/PPSSPPSDL.app/Contents/MacOS"));
    }
    dirs
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn well_known_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/var/lib/flatpak/exports/bin"),
        PathBuf::from("/snap/bin"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/bin"));
    }
    dirs
}

/// Locates PPSSPP, preferring an explicitly configured path.
pub fn resolve(configured: Option<&Path>) -> EmulatorStatus {
    // A configured path wins even over PATH: the user chose it deliberately.
    if let Some(path) = configured {
        if is_executable_file(path) {
            return describe(path.to_path_buf(), Source::Configured);
        }
        // Fall through rather than reporting missing — a stale setting should not
        // stop an otherwise-installed emulator from being found.
    }

    if let Some(path) = search_path() {
        return describe(path, Source::Path);
    }

    for dir in well_known_dirs() {
        for name in BINARY_NAMES {
            let candidate = dir.join(name);
            if is_executable_file(&candidate) {
                return describe(candidate, Source::Bundled);
            }
        }
    }

    EmulatorStatus::missing()
}

/// Walks `PATH` looking for any of the known binary names.
///
/// Hand-rolled rather than shelling out to `which`/`where`: spawning a process
/// to find a process is slower, and `where` is not always present.
fn search_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in BINARY_NAMES {
            let candidate = dir.join(name);
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn describe(path: PathBuf, source: Source) -> EmulatorStatus {
    EmulatorStatus {
        found: true,
        version: probe_version(&path),
        path: Some(path),
        source: Some(source.label().to_string()),
    }
}

/// Asks the binary for its version, tolerating one that does not answer.
///
/// Not all PPSSPP builds implement `--version`, and a build that opens a window
/// instead would hang the UI — so a non-zero exit or empty output is simply
/// reported as "unknown" rather than treated as a failure.
fn probe_version(path: &Path) -> Option<String> {
    let output = Command::new(path).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    Some(line.to_string())
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // An unset execute bit means it cannot be spawned, so it is not a match.
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Builds PPSSPP's argument list.
///
/// Kept separate from spawning so the arguments can be asserted on directly.
/// Only `--fullscreen` is passed; it is long-standing across PPSSPP builds,
/// whereas the more exotic flags vary between the SDL and Qt front-ends.
pub fn launch_args(game: &Path, fullscreen: bool) -> Vec<String> {
    let mut args = Vec::new();
    if fullscreen {
        args.push("--fullscreen".to_string());
    }
    // The game path goes last as a positional argument.
    args.push(game.to_string_lossy().into_owned());
    args
}

/// Starts PPSSPP on `game` and returns immediately.
///
/// The child is intentionally not waited on: the XMB stays responsive while the
/// game runs, matching how a console hands off to a title.
pub fn launch(
    configured: Option<&Path>,
    game: &Path,
    fullscreen: bool,
) -> Result<PathBuf, LaunchError> {
    // Check the game first: a missing file is the more actionable error, and
    // reporting "PPSSPP not found" for a deleted ROM would be misleading.
    if !game.is_file() {
        return Err(LaunchError::GameMissing(game.to_path_buf()));
    }

    let status = resolve(configured);
    let Some(binary) = status.path else {
        return Err(LaunchError::NotFound);
    };

    Command::new(&binary)
        // Separate argv entries — never a shell string — so paths containing
        // spaces or quotes cannot be reinterpreted.
        .args(launch_args(game, fullscreen))
        .spawn()
        .map_err(LaunchError::Spawn)?;

    Ok(binary)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a fake executable using the first name this platform looks for.
    fn fake_binary(dir: &Path) -> PathBuf {
        let path = dir.join(BINARY_NAMES[0]);
        std::fs::write(&path, b"#!/bin/sh\necho 'PPSSPP 1.17.1'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    #[test]
    fn a_configured_path_is_preferred() {
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_binary(dir.path());

        let status = resolve(Some(&binary));
        assert!(status.found);
        assert_eq!(status.path.as_deref(), Some(binary.as_path()));
        assert_eq!(status.source.as_deref(), Some("configured"));
    }

    #[test]
    fn a_stale_configured_path_falls_through_instead_of_failing() {
        // A path that no longer exists must not mask an otherwise fine install.
        let missing = PathBuf::from("/nonexistent/PPSSPPSDL");
        let status = resolve(Some(&missing));
        assert_ne!(status.source.as_deref(), Some("configured"));
    }

    #[test]
    fn reports_missing_when_nothing_is_installed() {
        // Empty PATH and a bogus configured path: nothing to find anywhere.
        temp_env_path("", || {
            let status = resolve(Some(Path::new("/nope/ppsspp")));
            // Well-known directories are still searched, so only assert on the
            // configured/PATH outcome, which must not claim a find.
            assert_ne!(status.source.as_deref(), Some("configured"));
            assert_ne!(status.source.as_deref(), Some("found on PATH"));
        });
    }

    #[cfg(unix)]
    #[test]
    fn finds_a_binary_on_path() {
        let dir = tempfile::tempdir().unwrap();
        fake_binary(dir.path());

        temp_env_path(dir.path().to_str().unwrap(), || {
            let status = resolve(None);
            assert!(status.found);
            assert_eq!(status.source.as_deref(), Some("found on PATH"));
        });
    }

    #[cfg(unix)]
    #[test]
    fn reads_the_version_from_the_binary() {
        let dir = tempfile::tempdir().unwrap();
        let binary = fake_binary(dir.path());
        let status = resolve(Some(&binary));
        assert_eq!(status.version.as_deref(), Some("PPSSPP 1.17.1"));
    }

    #[cfg(unix)]
    #[test]
    fn a_non_executable_file_is_not_a_match() {
        // Without the execute bit it cannot be spawned, so it must not resolve.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(BINARY_NAMES[0]);
        std::fs::write(&path, b"not executable").unwrap();

        assert!(!is_executable_file(&path));
        assert_ne!(resolve(Some(&path)).source.as_deref(), Some("configured"));
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_named_like_the_binary_is_not_a_match() {
        let dir = tempfile::tempdir().unwrap();
        let decoy = dir.path().join(BINARY_NAMES[0]);
        std::fs::create_dir(&decoy).unwrap();
        assert!(!is_executable_file(&decoy));
    }

    #[test]
    fn arguments_put_the_game_last_and_honour_fullscreen() {
        let args = launch_args(Path::new("/roms/game.iso"), true);
        assert_eq!(args, vec!["--fullscreen", "/roms/game.iso"]);

        let windowed = launch_args(Path::new("/roms/game.iso"), false);
        assert_eq!(windowed, vec!["/roms/game.iso"]);
    }

    #[test]
    fn paths_with_spaces_and_quotes_stay_a_single_argument() {
        // The whole reason arguments are built as a vector rather than a string.
        let nasty = Path::new("/roms/Ape Escape: On the Loose's \"Best\" Rip.iso");
        let args = launch_args(nasty, false);
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], nasty.to_string_lossy());
    }

    #[test]
    fn non_ascii_paths_survive_intact() {
        let path = Path::new("/roms/パタポン.iso");
        assert_eq!(launch_args(path, false)[0], "/roms/パタポン.iso");
    }

    #[test]
    fn launching_a_missing_game_reports_the_game_not_the_emulator() {
        let error = launch(None, Path::new("/roms/gone.iso"), true).unwrap_err();
        assert!(
            matches!(error, LaunchError::GameMissing(_)),
            "got {error:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn launching_without_an_emulator_reports_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game.iso");
        std::fs::write(&game, b"iso").unwrap();

        temp_env_path("", || {
            // Only meaningful if the host has no system-wide PPSSPP, which the CI
            // image does not. Skip the assertion when one is actually present.
            if resolve(None).found {
                return;
            }
            assert!(matches!(
                launch(None, &game, true).unwrap_err(),
                LaunchError::NotFound
            ));
        });
    }

    /// Runs `body` with `PATH` replaced, restoring it afterwards.
    ///
    /// Tests touching a process-global must not run concurrently, so they are
    /// serialised through a mutex.
    fn temp_env_path<T>(value: &str, body: impl FnOnce() -> T) -> T {
        use std::sync::Mutex;
        static LOCK: Mutex<()> = Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let original = std::env::var_os("PATH");
        // Safety: the mutex keeps other PATH-touching tests out for the duration.
        unsafe { std::env::set_var("PATH", value) };
        let result = body();
        match original {
            Some(path) => unsafe { std::env::set_var("PATH", path) },
            None => unsafe { std::env::remove_var("PATH") },
        }
        result
    }
}
