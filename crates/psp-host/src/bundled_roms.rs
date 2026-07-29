//! Installing the games that ship with the app.
//!
//! A shell for PSP games is unusable until it has one, and telling a first-time
//! user to go and find a ROM before they can see anything is a poor introduction.
//! So the installers carry a small set of games — homebrew the project is entitled
//! to distribute — and this copies them somewhere writable on request.
//!
//! Copying rather than scanning the install directory in place is deliberate. The
//! resource directory inside an `.app` bundle or under `Program Files` is
//! read-only, may be wiped by the next update, and is a strange thing to point a
//! user's library at. A folder under the app's data directory is theirs, survives
//! updates, and they can drop their own dumps into it beside the bundled ones.
//!
//! Nothing here decides *what* may be bundled — that is a licensing question
//! answered by what is committed to `demo-roms/`. This just moves files.

use std::path::{Path, PathBuf};

use psp_metadata::GameFormat;
use serde::Serialize;

/// A game found in the bundled set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BundledRom {
    pub file_name: String,
    pub size_bytes: u64,
}

/// The outcome of an install, detailed enough for the UI to say what happened.
#[derive(Debug, Clone, Default, Serialize)]
pub struct InstallReport {
    /// Where the games now live, and what gets added to the ROM paths.
    pub target: PathBuf,
    /// Games copied by this run.
    pub installed: Vec<String>,
    /// Games already present and identical, so left untouched.
    pub already_present: Vec<String>,
    /// Games that could not be copied, each with the reason.
    pub failed: Vec<InstallFailure>,
    pub bytes_copied: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallFailure {
    pub file_name: String,
    pub reason: String,
}

impl InstallReport {
    /// Whether the target folder now holds at least one game.
    ///
    /// True even when nothing was copied, because everything was already there —
    /// which is a success, not a no-op the UI should report as failure.
    pub fn is_populated(&self) -> bool {
        !self.installed.is_empty() || !self.already_present.is_empty()
    }
}

/// Lists the games in a bundled-ROM directory, sorted by name.
///
/// Ignores anything that is not a recognised game container, so the licence and
/// readme files that must accompany bundled homebrew do not get copied into the
/// user's library. A missing directory is not an error: a build may legitimately
/// ship with no games, and the UI reports that as "none bundled" rather than as a
/// broken install.
pub fn list(source: &Path) -> Vec<BundledRom> {
    let Ok(entries) = std::fs::read_dir(source) else {
        return Vec::new();
    };

    let mut roms: Vec<BundledRom> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            GameFormat::from_path(&path)?;
            // A directory could be named `Game.iso`; only files are games.
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some(BundledRom {
                file_name: entry.file_name().to_string_lossy().into_owned(),
                size_bytes: metadata.len(),
            })
        })
        .collect();

    // Stable order so the UI's count and the install log agree between runs.
    roms.sort_by(|a, b| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()));
    roms
}

/// Copies every bundled game into `target`, creating it if needed.
///
/// Idempotent and non-destructive:
///
/// - A file already there at the same size is left alone, so a second click is
///   free rather than rewriting hundreds of megabytes.
/// - A file there at a *different* size is overwritten, which repairs a copy
///   interrupted half-way.
/// - Nothing is ever deleted, so a user's own dumps in the same folder survive.
///
/// One game failing does not abandon the rest: a partially populated library is
/// more useful than an empty one, and the report names what failed.
pub fn install(source: &Path, target: &Path) -> std::io::Result<InstallReport> {
    let roms = list(source);
    let mut report = InstallReport {
        target: target.to_path_buf(),
        ..InstallReport::default()
    };
    if roms.is_empty() {
        return Ok(report);
    }

    // Only create the folder once there is something to put in it, so a build with
    // no bundled games does not leave an empty "Bundled Games" folder behind.
    std::fs::create_dir_all(target)?;

    for rom in roms {
        let from = source.join(&rom.file_name);
        let to = target.join(&rom.file_name);

        // Size is a cheap, good-enough identity check here: the source only
        // changes when the app is updated, and a same-name same-size file is the
        // one we would write. Hashing every ROM on every launch would cost far
        // more than it proves.
        if std::fs::metadata(&to).is_ok_and(|m| m.is_file() && m.len() == rom.size_bytes) {
            report.already_present.push(rom.file_name);
            continue;
        }

        match std::fs::copy(&from, &to) {
            Ok(bytes) => {
                report.bytes_copied += bytes;
                report.installed.push(rom.file_name);
            }
            Err(error) => report.failed.push(InstallFailure {
                file_name: rom.file_name,
                reason: error.to_string(),
            }),
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use psp_metadata::testkit::{IsoBuilder, PbpBuilder, SfoBuilder};

    /// A real, parseable PBP, so the scanner in the integration test has something
    /// genuine to read rather than a file of zeroes.
    fn homebrew_pbp(title: &str) -> Vec<u8> {
        PbpBuilder::new()
            .param_sfo(
                SfoBuilder::new()
                    .text("CATEGORY", "MG")
                    .text("TITLE", title)
                    .build(),
            )
            .data_psp(b"placeholder".to_vec())
            .build()
    }

    fn source_with(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in files {
            std::fs::write(dir.path().join(name), bytes).unwrap();
        }
        dir
    }

    #[test]
    fn a_missing_source_directory_lists_nothing_rather_than_failing() {
        // A build may ship with no bundled games at all.
        assert!(list(Path::new("/definitely/not/here")).is_empty());
    }

    #[test]
    fn installing_from_a_missing_source_creates_nothing() {
        let target = tempfile::tempdir().unwrap();
        let nested = target.path().join("Bundled Games");
        let report = install(Path::new("/definitely/not/here"), &nested).unwrap();

        assert!(report.installed.is_empty());
        assert!(!report.is_populated());
        assert!(
            !nested.exists(),
            "an empty bundle should not leave a folder behind"
        );
    }

    #[test]
    fn lists_only_game_containers() {
        // The licence and readme that must accompany bundled homebrew live in the
        // same folder, and copying them into the user's ROM library is wrong.
        let source = source_with(&[
            ("Batman.pbp", b"pbp"),
            ("Homebrew.iso", b"iso"),
            ("Squashed.cso", b"cso"),
            ("Bare.elf", b"elf"),
            ("LICENSE.txt", b"licence"),
            ("README.md", b"readme"),
            ("cover.png", b"png"),
        ]);

        let names: Vec<_> = list(source.path())
            .into_iter()
            .map(|r| r.file_name)
            .collect();
        assert_eq!(
            names,
            ["Bare.elf", "Batman.pbp", "Homebrew.iso", "Squashed.cso"]
        );
    }

    #[test]
    fn a_directory_named_like_a_game_is_not_listed() {
        let source = tempfile::tempdir().unwrap();
        std::fs::create_dir(source.path().join("NotAGame.iso")).unwrap();
        assert!(list(source.path()).is_empty());
    }

    #[test]
    fn extension_matching_ignores_case() {
        let source = source_with(&[("SHOUTING.ISO", b"iso"), ("Mixed.Pbp", b"pbp")]);
        assert_eq!(list(source.path()).len(), 2);
    }

    #[test]
    fn copies_every_bundled_game_into_the_target() {
        let source = source_with(&[
            ("Batman.pbp", homebrew_pbp("Batman").as_slice()),
            ("LICENSE.txt", b"licence"),
        ]);
        let target = tempfile::tempdir().unwrap();
        let into = target.path().join("Bundled Games");

        let report = install(source.path(), &into).unwrap();

        assert_eq!(report.installed, ["Batman.pbp"]);
        assert!(report.failed.is_empty());
        assert!(report.is_populated());
        assert!(into.join("Batman.pbp").exists());
        assert!(
            !into.join("LICENSE.txt").exists(),
            "the licence belongs with the bundle, not in the ROM folder"
        );
        assert_eq!(report.bytes_copied, homebrew_pbp("Batman").len() as u64);
    }

    #[test]
    fn a_second_install_copies_nothing_and_still_reports_success() {
        let source = source_with(&[("Batman.pbp", homebrew_pbp("Batman").as_slice())]);
        let target = tempfile::tempdir().unwrap();

        install(source.path(), target.path()).unwrap();
        let again = install(source.path(), target.path()).unwrap();

        assert!(again.installed.is_empty());
        assert_eq!(again.already_present, ["Batman.pbp"]);
        assert_eq!(again.bytes_copied, 0, "no rewriting an identical file");
        assert!(
            again.is_populated(),
            "everything already present is success, not failure"
        );
    }

    #[test]
    fn a_truncated_previous_copy_is_repaired() {
        let bytes = homebrew_pbp("Batman");
        let source = source_with(&[("Batman.pbp", bytes.as_slice())]);
        let target = tempfile::tempdir().unwrap();
        // Simulate an install interrupted part-way through.
        std::fs::write(target.path().join("Batman.pbp"), &bytes[..8]).unwrap();

        let report = install(source.path(), target.path()).unwrap();

        assert_eq!(report.installed, ["Batman.pbp"]);
        assert_eq!(
            std::fs::read(target.path().join("Batman.pbp")).unwrap(),
            bytes
        );
    }

    #[test]
    fn a_users_own_games_in_the_target_are_left_alone() {
        let source = source_with(&[("Batman.pbp", homebrew_pbp("Batman").as_slice())]);
        let target = tempfile::tempdir().unwrap();
        let mine = target.path().join("My Dump.iso");
        std::fs::write(&mine, b"my own dump").unwrap();

        install(source.path(), target.path()).unwrap();

        assert_eq!(std::fs::read(&mine).unwrap(), b"my own dump");
    }

    #[test]
    fn installed_games_are_then_found_by_the_library_scanner() {
        // The whole point of the copy: the shell must see the games afterwards.
        let source = source_with(&[
            ("Batman.pbp", homebrew_pbp("Batman Homebrew").as_slice()),
            (
                "Adventure.iso",
                IsoBuilder::new()
                    .param_sfo(
                        SfoBuilder::new()
                            .text("CATEGORY", "UG")
                            .text("TITLE", "Adventure")
                            .build(),
                    )
                    .build()
                    .as_slice(),
            ),
            ("LICENSE.txt", b"licence"),
        ]);
        let target = tempfile::tempdir().unwrap();

        let report = install(source.path(), target.path()).unwrap();
        assert_eq!(report.installed.len(), 2);

        let scan = psp_metadata::scan_library(&[target.path().to_path_buf()]);
        let mut titles: Vec<_> = scan.games.iter().map(|g| g.title.as_str()).collect();
        titles.sort_unstable();
        assert_eq!(titles, ["Adventure", "Batman Homebrew"]);
        assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    }
}
