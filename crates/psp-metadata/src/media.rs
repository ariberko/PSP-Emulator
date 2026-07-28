//! Scanning the Photo, Music and Video folders.
//!
//! A real PSP's memory stick has `PHOTO`, `MUSIC` and `VIDEO` directories, and the
//! XMB's three non-game categories list what is in them. Reproducing that needs
//! no format parsing — the webview decodes JPEG, MP3 and MP4 natively — so this is
//! only classification: walk the configured roots and sort files into the three
//! categories by extension.
//!
//! Classification is by extension rather than by sniffing content. A wrong guess
//! here costs nothing (the player simply refuses a file the webview cannot
//! decode), and reading headers from every file in a photo library would make
//! scanning far slower for no real gain.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Recursion depth below each configured root, matching the game scanner's.
pub const DEFAULT_MAX_DEPTH: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Photo,
    Music,
    Video,
}

impl MediaKind {
    pub fn label(&self) -> &'static str {
        match self {
            MediaKind::Photo => "Photo",
            MediaKind::Music => "Music",
            MediaKind::Video => "Video",
        }
    }
}

/// Extensions the webview can be expected to decode.
///
/// Deliberately conservative: listing a format the platform cannot play would put
/// an entry in the list that fails when selected, which is worse than omitting it.
const PHOTO_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "avif"];
const MUSIC_EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "wav", "ogg", "oga", "opus", "flac"];
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "m4v", "webm", "ogv", "mov"];

/// One playable or viewable file.
#[derive(Debug, Clone, Serialize)]
pub struct MediaItem {
    /// Stable id for UI keying: the full path.
    pub id: String,
    /// File name without its extension, which is what the XMB shows.
    pub title: String,
    pub path: PathBuf,
    pub kind: MediaKind,
    pub size_bytes: u64,
    /// Lower-case extension, shown as the format in the item's second line.
    pub extension: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MediaScan {
    pub photos: Vec<MediaItem>,
    pub music: Vec<MediaItem>,
    pub videos: Vec<MediaItem>,
    /// Configured roots that do not exist, so the UI can say so.
    pub missing_roots: Vec<PathBuf>,
}

impl MediaScan {
    pub fn is_empty(&self) -> bool {
        self.photos.is_empty() && self.music.is_empty() && self.videos.is_empty()
    }

    pub fn total(&self) -> usize {
        self.photos.len() + self.music.len() + self.videos.len()
    }
}

/// Scans every root for media.
///
/// Never fails as a whole: an unreadable folder is skipped and the rest still
/// loads, matching how the game scanner behaves.
pub fn scan_media(roots: &[PathBuf]) -> MediaScan {
    scan_media_with_depth(roots, DEFAULT_MAX_DEPTH)
}

pub fn scan_media_with_depth(roots: &[PathBuf], max_depth: usize) -> MediaScan {
    let mut scan = MediaScan::default();

    for root in roots {
        if !root.is_dir() {
            scan.missing_roots.push(root.clone());
            continue;
        }
        visit(root, max_depth, &mut scan);
    }

    // Sorted the way the XMB lists them: by title, case-insensitively, with the
    // path as a tiebreak so the order is stable across scans.
    for list in [&mut scan.photos, &mut scan.music, &mut scan.videos] {
        list.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then_with(|| a.path.cmp(&b.path))
        });
    }
    scan
}

fn visit(dir: &Path, depth_left: usize, scan: &mut MediaScan) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        // Unreadable folders are skipped silently: unlike a missing game, a photo
        // folder without permissions is not something the user asked about.
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            if is_ignored_dir(&path) {
                continue;
            }
            if depth_left > 0 {
                visit(&path, depth_left - 1, scan);
            }
            continue;
        }

        if let Some(item) = classify(&path) {
            match item.kind {
                MediaKind::Photo => scan.photos.push(item),
                MediaKind::Music => scan.music.push(item),
                MediaKind::Video => scan.videos.push(item),
            }
        }
    }
}

/// Sorts a file into a category, or `None` if it is not media.
pub fn classify(path: &Path) -> Option<MediaItem> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();

    let kind = if PHOTO_EXTENSIONS.contains(&extension.as_str()) {
        MediaKind::Photo
    } else if MUSIC_EXTENSIONS.contains(&extension.as_str()) {
        MediaKind::Music
    } else if VIDEO_EXTENSIONS.contains(&extension.as_str()) {
        MediaKind::Video
    } else {
        return None;
    };

    // Metadata may fail on a file that vanished mid-scan; treat it as absent
    // rather than reporting a zero-byte entry.
    let size_bytes = std::fs::metadata(path).ok()?.len();

    let title = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Untitled".to_string());

    Some(MediaItem {
        id: path.to_string_lossy().into_owned(),
        title,
        path: path.to_path_buf(),
        kind,
        size_bytes,
        extension,
    })
}

fn is_ignored_dir(path: &Path) -> bool {
    // PPSSPP's own folders hold game data and save states, not the user's media.
    const IGNORED: &[&str] = &["SAVEDATA", "PPSSPP_STATE", "SYSTEM", "TEXTURES", "GAME"];
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name.starts_with('.') || IGNORED.iter().any(|i| name.eq_ignore_ascii_case(i)))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"x").unwrap();
    }

    #[test]
    fn sorts_files_into_the_three_categories() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["holiday.jpg", "song.mp3", "clip.mp4", "notes.txt"] {
            touch(dir.path(), name);
        }

        let scan = scan_media(&[dir.path().to_path_buf()]);
        assert_eq!(scan.photos.len(), 1);
        assert_eq!(scan.music.len(), 1);
        assert_eq!(scan.videos.len(), 1);
        assert_eq!(scan.total(), 3, "the .txt must be ignored");
    }

    #[test]
    fn titles_drop_the_extension() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "A Day At The Beach.jpg");
        let scan = scan_media(&[dir.path().to_path_buf()]);
        assert_eq!(scan.photos[0].title, "A Day At The Beach");
        assert_eq!(scan.photos[0].extension, "jpg");
    }

    #[test]
    fn extension_matching_is_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.JPG", "b.Mp3", "c.MP4"] {
            touch(dir.path(), name);
        }
        assert_eq!(scan_media(&[dir.path().to_path_buf()]).total(), 3);
    }

    #[test]
    fn recurses_into_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("2007").join("Summer");
        std::fs::create_dir_all(&nested).unwrap();
        touch(&nested, "beach.jpg");

        assert_eq!(scan_media(&[dir.path().to_path_buf()]).photos.len(), 1);
    }

    #[test]
    fn honours_the_depth_limit() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();
        touch(&deep, "deep.jpg");

        assert_eq!(
            scan_media_with_depth(&[dir.path().to_path_buf()], 1).total(),
            0
        );
        assert_eq!(
            scan_media_with_depth(&[dir.path().to_path_buf()], 3).total(),
            1
        );
    }

    #[test]
    fn skips_emulator_folders() {
        // A memory-stick root is a plausible media root, and its PPSSPP folders
        // hold state files rather than the user's own media.
        let dir = tempfile::tempdir().unwrap();
        for folder in ["PPSSPP_STATE", "SAVEDATA", "SYSTEM"] {
            let sub = dir.path().join(folder);
            std::fs::create_dir_all(&sub).unwrap();
            touch(&sub, "thumb.jpg");
        }
        assert_eq!(scan_media(&[dir.path().to_path_buf()]).total(), 0);
    }

    #[test]
    fn skips_hidden_folders() {
        let dir = tempfile::tempdir().unwrap();
        let hidden = dir.path().join(".thumbnails");
        std::fs::create_dir_all(&hidden).unwrap();
        touch(&hidden, "cache.jpg");
        assert_eq!(scan_media(&[dir.path().to_path_buf()]).total(), 0);
    }

    #[test]
    fn reports_roots_that_do_not_exist() {
        let scan = scan_media(&[PathBuf::from("/nonexistent/media")]);
        assert_eq!(
            scan.missing_roots,
            vec![PathBuf::from("/nonexistent/media")]
        );
        assert!(scan.is_empty());
    }

    #[test]
    fn an_empty_root_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let scan = scan_media(&[dir.path().to_path_buf()]);
        assert!(scan.is_empty());
        assert!(scan.missing_roots.is_empty());
    }

    #[test]
    fn sort_order_is_case_insensitive_and_stable() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["zebra.jpg", "Apple.jpg", "mango.jpg"] {
            touch(dir.path(), name);
        }
        let titles: Vec<_> = scan_media(&[dir.path().to_path_buf()])
            .photos
            .into_iter()
            .map(|p| p.title)
            .collect();
        assert_eq!(titles, vec!["Apple", "mango", "zebra"]);
    }

    #[test]
    fn a_file_with_no_extension_is_not_media() {
        assert!(classify(Path::new("/tmp/README")).is_none());
    }

    #[test]
    fn covers_the_formats_a_webview_can_decode() {
        let dir = tempfile::tempdir().unwrap();
        // One of each declared extension, to catch a typo in the tables.
        for ext in PHOTO_EXTENSIONS {
            touch(dir.path(), &format!("photo.{ext}"));
        }
        for ext in MUSIC_EXTENSIONS {
            touch(dir.path(), &format!("track.{ext}"));
        }
        for ext in VIDEO_EXTENSIONS {
            touch(dir.path(), &format!("movie.{ext}"));
        }

        let scan = scan_media(&[dir.path().to_path_buf()]);
        assert_eq!(scan.photos.len(), PHOTO_EXTENSIONS.len());
        assert_eq!(scan.music.len(), MUSIC_EXTENSIONS.len());
        assert_eq!(scan.videos.len(), VIDEO_EXTENSIONS.len());
    }

    #[test]
    fn no_extension_appears_in_more_than_one_category() {
        // Overlap would make classification order-dependent and surprising.
        for ext in PHOTO_EXTENSIONS {
            assert!(!MUSIC_EXTENSIONS.contains(ext), "{ext} in photo and music");
            assert!(!VIDEO_EXTENSIONS.contains(ext), "{ext} in photo and video");
        }
        for ext in MUSIC_EXTENSIONS {
            assert!(!VIDEO_EXTENSIONS.contains(ext), "{ext} in music and video");
        }
    }
}
