//! Turning a file on disk into something the XMB can show.

use std::path::{Path, PathBuf};

use serde::{Serialize, Serializer};

use crate::cso::CsoReader;
use crate::iso::{FileSource, IsoReader};
use crate::pbp::Pbp;
use crate::sfo::Sfo;

/// Container a title is stored in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GameFormat {
    /// Raw UMD rip.
    Iso,
    /// Compressed UMD rip.
    Cso,
    /// PSN download or packaged homebrew.
    Pbp,
    /// Bare homebrew executable.
    Elf,
}

impl GameFormat {
    pub fn label(&self) -> &'static str {
        match self {
            GameFormat::Iso => "ISO",
            GameFormat::Cso => "CSO",
            GameFormat::Pbp => "PBP",
            GameFormat::Elf => "ELF",
        }
    }
}

/// Categories that are not launchable titles. `MS` is save data and `PP` is a
/// theme; both routinely sit in the same folder as games.
const NON_GAME_CATEGORIES: &[&str] = &["MS", "PP"];

/// One entry in the game list.
#[derive(Debug, Clone, Serialize)]
pub struct Game {
    /// Stable identifier for UI keying: the disc ID when known, else the path.
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub format: GameFormat,
    pub size_bytes: u64,
    pub disc_id: Option<String>,
    pub disc_version: Option<String>,
    pub category: Option<String>,
    /// Minimum firmware the title declares, e.g. `"6.60"`.
    pub system_version: Option<String>,
    pub parental_level: Option<u32>,
    /// 144×80 list icon, serialised as a `data:` URL the webview can use directly.
    #[serde(serialize_with = "as_png_data_url", rename = "icon")]
    pub icon_png: Option<Vec<u8>>,
    /// 480×272 background shown behind the selected entry.
    #[serde(serialize_with = "as_png_data_url", rename = "background")]
    pub background_png: Option<Vec<u8>>,
}

impl Game {
    /// Reads whatever metadata `path`'s format exposes.
    ///
    /// Metadata is best-effort by design: a homebrew ELF has no PARAM.SFO, and a
    /// half-copied ISO should still appear in the list rather than vanish. When
    /// parsing fails the file name becomes the title.
    pub fn probe(path: &Path) -> std::io::Result<Option<Self>> {
        let Some(format) = detect_format(path) else {
            return Ok(None);
        };
        let size_bytes = std::fs::metadata(path)?.len();

        let (sfo, icon_png, background_png) = match format {
            GameFormat::Pbp => read_pbp(path)?,
            GameFormat::Iso | GameFormat::Cso => read_disc(path, format),
            GameFormat::Elf => (None, None, None),
        };

        let title = sfo
            .as_ref()
            .and_then(Sfo::title)
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| file_stem(path));

        let disc_id = sfo.as_ref().and_then(Sfo::disc_id).map(str::to_string);
        let category = sfo.as_ref().and_then(Sfo::category).map(str::to_string);

        let id = disc_id
            .clone()
            .unwrap_or_else(|| path.to_string_lossy().into_owned());

        Ok(Some(Self {
            id,
            title,
            path: path.to_path_buf(),
            format,
            size_bytes,
            disc_id,
            disc_version: sfo.as_ref().and_then(Sfo::disc_version).map(str::to_string),
            category,
            system_version: sfo
                .as_ref()
                .and_then(Sfo::system_version)
                .map(str::to_string),
            parental_level: sfo.as_ref().and_then(Sfo::parental_level),
            icon_png,
            background_png,
        }))
    }

    /// Whether this belongs in the XMB game list, as opposed to being save data
    /// or a theme that happens to share the folder.
    pub fn is_launchable(&self) -> bool {
        match self.category.as_deref() {
            Some(category) => !NON_GAME_CATEGORIES.contains(&category),
            None => true,
        }
    }
}

fn detect_format(path: &Path) -> Option<GameFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "iso" => Some(GameFormat::Iso),
        "cso" => Some(GameFormat::Cso),
        "pbp" => Some(GameFormat::Pbp),
        "elf" | "prx" => Some(GameFormat::Elf),
        _ => None,
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_string())
}

type Metadata = (Option<Sfo>, Option<Vec<u8>>, Option<Vec<u8>>);

fn read_pbp(path: &Path) -> std::io::Result<Metadata> {
    let bytes = std::fs::read(path)?;
    let Ok(pbp) = Pbp::parse(&bytes) else {
        return Ok((None, None, None));
    };
    let sfo = pbp.param_sfo().and_then(|b| Sfo::parse(b).ok());
    Ok((
        sfo,
        pbp.icon0().map(<[u8]>::to_vec),
        pbp.pic1().map(<[u8]>::to_vec),
    ))
}

/// Pulls metadata out of an ISO or CSO.
///
/// Infallible by intent: an unreadable disc still yields a list entry named
/// after its file, which is far better UX than a title silently disappearing.
fn read_disc(path: &Path, format: GameFormat) -> Metadata {
    fn from_reader<S: crate::iso::ReadAt>(source: S) -> Metadata {
        let Ok(iso) = IsoReader::new(source) else {
            return (None, None, None);
        };
        let sfo = iso
            .read_file("PSP_GAME/PARAM.SFO")
            .ok()
            .flatten()
            .and_then(|b| Sfo::parse(&b).ok());
        let icon = iso.read_file("PSP_GAME/ICON0.PNG").ok().flatten();
        let background = iso.read_file("PSP_GAME/PIC1.PNG").ok().flatten();
        (sfo, icon, background)
    }

    let Ok(file) = FileSource::open(path) else {
        return (None, None, None);
    };
    match format {
        GameFormat::Cso => match CsoReader::new(file) {
            Ok(cso) => from_reader(cso),
            Err(_) => (None, None, None),
        },
        _ => from_reader(file),
    }
}

/// Serialises PNG bytes as a `data:` URL so the webview can use the value as an
/// `img` source with no extra IPC round trip.
fn as_png_data_url<S: Serializer>(bytes: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
    match bytes {
        Some(bytes) => s.serialize_str(&format!("data:image/png;base64,{}", base64(bytes))),
        None => s.serialize_none(),
    }
}

/// Minimal base64 encoder. The only thing this crate needs an encoder for is
/// icon data URLs, which is not worth a dependency.
fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let bits = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[(bits >> 18 & 0x3F) as usize] as char);
        out.push(ALPHABET[(bits >> 12 & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(bits >> 6 & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(bits & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        // RFC 4648 test vectors, including every padding case.
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_handles_high_bytes() {
        assert_eq!(base64(&[0xFF, 0xFE, 0xFD]), "//79");
        assert_eq!(base64(&[0x00, 0x00, 0x00]), "AAAA");
    }

    #[test]
    fn format_detection_is_case_insensitive() {
        assert_eq!(detect_format(Path::new("g.ISO")), Some(GameFormat::Iso));
        assert_eq!(detect_format(Path::new("g.Cso")), Some(GameFormat::Cso));
        assert_eq!(detect_format(Path::new("g.pbp")), Some(GameFormat::Pbp));
        assert_eq!(detect_format(Path::new("g.prx")), Some(GameFormat::Elf));
        assert_eq!(detect_format(Path::new("notes.txt")), None);
        assert_eq!(detect_format(Path::new("no-extension")), None);
    }

    #[test]
    fn save_data_and_themes_are_not_launchable() {
        let mut game = fake_game();
        game.category = Some("MS".to_string());
        assert!(!game.is_launchable());
        game.category = Some("PP".to_string());
        assert!(!game.is_launchable());
    }

    #[test]
    fn games_and_metadata_free_homebrew_are_launchable() {
        let mut game = fake_game();
        game.category = Some("UG".to_string());
        assert!(game.is_launchable());
        game.category = Some("MG".to_string());
        assert!(game.is_launchable());
        // Homebrew ELFs have no PARAM.SFO and so no category at all.
        game.category = None;
        assert!(game.is_launchable());
    }

    #[test]
    fn icons_serialise_as_data_urls() {
        let mut game = fake_game();
        game.icon_png = Some(b"foobar".to_vec());
        let json = serde_json::to_value(&game).unwrap();
        assert_eq!(json["icon"], "data:image/png;base64,Zm9vYmFy");
        // An absent icon must be null, not an empty data URL the UI would render.
        assert!(json["background"].is_null());
    }

    fn fake_game() -> Game {
        Game {
            id: "UCUS98618".into(),
            title: "Daxter".into(),
            path: PathBuf::from("/roms/daxter.iso"),
            format: GameFormat::Iso,
            size_bytes: 1024,
            disc_id: Some("UCUS98618".into()),
            disc_version: None,
            category: Some("UG".into()),
            system_version: None,
            parental_level: None,
            icon_png: None,
            background_png: None,
        }
    }
}
