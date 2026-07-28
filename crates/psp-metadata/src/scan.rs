//! Walking ROM folders to build the game list.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::game::Game;

/// How deep to recurse below a configured root.
///
/// People organise ROMs in per-title or per-genre subfolders, so a flat scan is
/// not enough — but an unbounded walk of a whole drive is a footgun.
pub const DEFAULT_MAX_DEPTH: usize = 4;

/// A file that looked like a game but could not be read.
///
/// Surfaced rather than swallowed: "why is this game missing" is otherwise
/// impossible to answer from the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ScanProblem {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LibraryScan {
    pub games: Vec<Game>,
    pub problems: Vec<ScanProblem>,
    /// Roots that were configured but do not exist, so the UI can say so.
    pub missing_roots: Vec<PathBuf>,
}

/// Scans every root, returning the combined list sorted for display.
///
/// Never fails as a whole: an unreadable folder or file becomes a problem entry
/// and the rest of the library still loads.
pub fn scan_library(roots: &[PathBuf]) -> LibraryScan {
    scan_library_with_depth(roots, DEFAULT_MAX_DEPTH)
}

pub fn scan_library_with_depth(roots: &[PathBuf], max_depth: usize) -> LibraryScan {
    let mut scan = LibraryScan::default();

    for root in roots {
        if !root.is_dir() {
            scan.missing_roots.push(root.clone());
            continue;
        }
        visit(root, max_depth, &mut scan);
    }

    // Sort the way the XMB does: by title, case-insensitively, with the path as
    // a tiebreak so the order is stable across scans.
    scan.games.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.path.cmp(&b.path))
    });
    scan.problems.sort_by(|a, b| a.path.cmp(&b.path));
    scan
}

fn visit(dir: &Path, depth_left: usize, scan: &mut LibraryScan) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            scan.problems.push(ScanProblem {
                path: dir.to_path_buf(),
                reason: format!("could not read directory: {e}"),
            });
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            // Skip PPSSPP's own state folders, which contain PBPs that are save
            // data rather than games.
            if is_ignored_dir(&path) {
                continue;
            }
            if depth_left > 0 {
                visit(&path, depth_left - 1, scan);
            }
            continue;
        }

        match Game::probe(&path) {
            Ok(Some(game)) => {
                if game.is_launchable() {
                    scan.games.push(game);
                }
            }
            // Not a recognised extension — the overwhelmingly common case.
            Ok(None) => {}
            Err(e) => scan.problems.push(ScanProblem {
                path,
                reason: e.to_string(),
            }),
        }
    }
}

fn is_ignored_dir(path: &Path) -> bool {
    const IGNORED: &[&str] = &["SAVEDATA", "PSP", "SYSTEM", "TEXTURES", "SHADERS"];
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name.starts_with('.') || IGNORED.iter().any(|i| name.eq_ignore_ascii_case(i)))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{cso_store_only, tiny_png, IsoBuilder, PbpBuilder, SfoBuilder};

    fn iso_bytes(title: &str, disc_id: &str) -> Vec<u8> {
        IsoBuilder::new()
            .volume_id(disc_id)
            .param_sfo(
                SfoBuilder::new()
                    .text("TITLE", title)
                    .text("DISC_ID", disc_id)
                    .text("CATEGORY", "UG")
                    .build(),
            )
            .icon0(tiny_png(144, 80))
            .build()
    }

    #[test]
    fn reads_titles_and_icons_out_of_a_mixed_library() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("daxter.iso"),
            iso_bytes("Daxter", "UCUS98618"),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("patapon.cso"),
            cso_store_only(&iso_bytes("Patapon", "UCUS98751"), 2048),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("homebrew.pbp"),
            PbpBuilder::new()
                .param_sfo(SfoBuilder::new().text("TITLE", "Cave Story").build())
                .icon0(tiny_png(144, 80))
                .build(),
        )
        .unwrap();
        // Files that are not games at all must be ignored silently.
        std::fs::write(dir.path().join("readme.txt"), b"hello").unwrap();

        let scan = scan_library(&[dir.path().to_path_buf()]);
        let titles: Vec<_> = scan.games.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, vec!["Cave Story", "Daxter", "Patapon"]);
        assert!(scan.problems.is_empty(), "{:?}", scan.problems);

        let daxter = scan.games.iter().find(|g| g.title == "Daxter").unwrap();
        assert_eq!(daxter.disc_id.as_deref(), Some("UCUS98618"));
        assert_eq!(daxter.format.label(), "ISO");
        assert!(daxter.icon_png.is_some(), "icon should come out of the ISO");

        // The CSO must be read through the same walker as the plain ISO.
        let patapon = scan.games.iter().find(|g| g.title == "Patapon").unwrap();
        assert_eq!(patapon.format.label(), "CSO");
        assert!(patapon.icon_png.is_some());
    }

    #[test]
    fn recurses_into_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("Racing").join("Wipeout");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            nested.join("wipeout.iso"),
            iso_bytes("Wipeout Pure", "UCUS98612"),
        )
        .unwrap();

        let scan = scan_library(&[dir.path().to_path_buf()]);
        assert_eq!(scan.games.len(), 1);
        assert_eq!(scan.games[0].title, "Wipeout Pure");
    }

    #[test]
    fn honours_the_depth_limit() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("game.iso"), iso_bytes("Too Deep", "UCUS00001")).unwrap();

        assert_eq!(
            scan_library_with_depth(&[dir.path().to_path_buf()], 1)
                .games
                .len(),
            0
        );
        assert_eq!(
            scan_library_with_depth(&[dir.path().to_path_buf()], 3)
                .games
                .len(),
            1
        );
    }

    #[test]
    fn skips_save_data_and_emulator_folders() {
        let dir = tempfile::tempdir().unwrap();
        let savedata = dir.path().join("SAVEDATA");
        std::fs::create_dir_all(&savedata).unwrap();
        std::fs::write(
            savedata.join("save.pbp"),
            PbpBuilder::new()
                .param_sfo(SfoBuilder::new().text("TITLE", "Save Slot 1").build())
                .build(),
        )
        .unwrap();

        assert!(scan_library(&[dir.path().to_path_buf()]).games.is_empty());
    }

    #[test]
    fn excludes_save_data_pbps_sitting_next_to_games() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("game.iso"), iso_bytes("A Game", "UCUS1")).unwrap();
        std::fs::write(
            dir.path().join("save.pbp"),
            PbpBuilder::new()
                .param_sfo(
                    SfoBuilder::new()
                        .text("TITLE", "A Game Save")
                        .text("CATEGORY", "MS")
                        .build(),
                )
                .build(),
        )
        .unwrap();

        let titles: Vec<_> = scan_library(&[dir.path().to_path_buf()])
            .games
            .iter()
            .map(|g| g.title.clone())
            .collect();
        assert_eq!(titles, vec!["A Game"]);
    }

    #[test]
    fn unreadable_images_still_appear_named_after_their_file() {
        // Losing a title from the list is worse than showing it with a filename.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Truncated Game.iso"), b"not really an iso").unwrap();

        let scan = scan_library(&[dir.path().to_path_buf()]);
        assert_eq!(scan.games.len(), 1);
        assert_eq!(scan.games[0].title, "Truncated Game");
        assert!(scan.games[0].icon_png.is_none());
    }

    #[test]
    fn reports_roots_that_do_not_exist() {
        let scan = scan_library(&[PathBuf::from("/nonexistent/roms")]);
        assert_eq!(scan.missing_roots, vec![PathBuf::from("/nonexistent/roms")]);
        assert!(scan.games.is_empty());
    }

    #[test]
    fn an_empty_library_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let scan = scan_library(&[dir.path().to_path_buf()]);
        assert!(scan.games.is_empty());
        assert!(scan.problems.is_empty());
        assert!(scan.missing_roots.is_empty());
    }

    #[test]
    fn sort_order_is_stable_and_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        for (file, title) in [("z.iso", "apple"), ("a.iso", "Banana"), ("m.iso", "Cherry")] {
            std::fs::write(dir.path().join(file), iso_bytes(title, "UCUS1")).unwrap();
        }
        let titles: Vec<_> = scan_library(&[dir.path().to_path_buf()])
            .games
            .iter()
            .map(|g| g.title.clone())
            .collect();
        assert_eq!(titles, vec!["apple", "Banana", "Cherry"]);
    }
}
