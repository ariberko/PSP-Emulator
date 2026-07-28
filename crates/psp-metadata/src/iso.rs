//! ISO9660 reader, scoped to what a UMD rip needs.
//!
//! A PSP disc image is a plain ISO9660 filesystem. Everything the XMB wants
//! lives at two fixed paths, `PSP_GAME/PARAM.SFO` and `PSP_GAME/ICON0.PNG`, so
//! this reader only implements directory-record walking — no Joliet, no Rock
//! Ridge, no path tables. Those are irrelevant for PSP images and each one is
//! more surface area to get wrong.
//!
//! Reads go through [`ReadAt`] so the same walker serves a plain `.iso` on disk
//! and a `.cso` whose blocks are decompressed on demand.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub const SECTOR_LEN: usize = 2048;

/// The sector holding the Primary Volume Descriptor, fixed by the standard.
const PVD_SECTOR: u64 = 16;
/// Offset of the root directory record within the PVD.
const PVD_ROOT_RECORD: usize = 156;
/// Directory record flag bit marking a directory.
const FLAG_DIRECTORY: u8 = 0x02;

/// Random-access byte source.
///
/// Deliberately not `Read + Seek`: a CSO block cache wants `&self` reads, and
/// threading `&mut` through the walker for something that is logically a
/// read-only view makes the call sites worse.
pub trait ReadAt {
    /// Fills `buf` from `offset`, erroring if the source ends first.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()>;

    /// Total readable length.
    fn size(&self) -> u64;
}

impl ReadAt for [u8] {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let start = offset as usize;
        let end = start
            .checked_add(buf.len())
            .filter(|end| *end <= self.len())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::UnexpectedEof, "read past end of image")
            })?;
        buf.copy_from_slice(&self[start..end]);
        Ok(())
    }

    fn size(&self) -> u64 {
        self.len() as u64
    }
}

impl ReadAt for Vec<u8> {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        self.as_slice().read_at(offset, buf)
    }

    fn size(&self) -> u64 {
        self.len() as u64
    }
}

/// A file opened for positioned reads.
///
/// `File::read_at` is Unix-only, so this keeps a `RefCell` seek cursor to stay
/// portable to the Windows builds the desktop app ships.
pub struct FileSource {
    file: std::cell::RefCell<File>,
    size: u64,
}

impl FileSource {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            file: std::cell::RefCell::new(file),
            size,
        })
    }
}

impl ReadAt for FileSource {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buf)
    }

    fn size(&self) -> u64 {
        self.size
    }
}

/// One entry in a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsoEntry {
    /// Identifier with any `;1` version suffix removed.
    pub name: String,
    pub lba: u32,
    pub len: u32,
    pub is_dir: bool,
}

#[derive(Debug)]
pub enum IsoError {
    /// No `CD001` Primary Volume Descriptor where one is required.
    NotIso9660,
    /// A record declares a size or location the image cannot satisfy.
    Corrupt(&'static str),
    Io(io::Error),
}

impl std::fmt::Display for IsoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IsoError::NotIso9660 => write!(f, "not an ISO9660 image"),
            IsoError::Corrupt(what) => write!(f, "corrupt ISO9660 image: {what}"),
            IsoError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for IsoError {}

impl From<io::Error> for IsoError {
    fn from(e: io::Error) -> Self {
        IsoError::Io(e)
    }
}

pub struct IsoReader<S: ReadAt> {
    source: S,
    root: IsoEntry,
    volume_id: String,
}

impl<S: ReadAt> IsoReader<S> {
    pub fn new(source: S) -> Result<Self, IsoError> {
        let mut pvd = [0u8; SECTOR_LEN];
        source
            .read_at(PVD_SECTOR * SECTOR_LEN as u64, &mut pvd)
            .map_err(|_| IsoError::NotIso9660)?;

        if pvd[0] != 1 || &pvd[1..6] != b"CD001" {
            return Err(IsoError::NotIso9660);
        }

        let volume_id = String::from_utf8_lossy(&pvd[40..72]).trim().to_string();
        let root = parse_dir_record(&pvd[PVD_ROOT_RECORD..])
            .ok_or(IsoError::Corrupt("root directory record"))?;

        Ok(Self {
            source,
            root,
            volume_id,
        })
    }

    /// Volume identifier from the PVD.
    pub fn volume_id(&self) -> &str {
        &self.volume_id
    }

    /// Lists a directory's entries, skipping the `.` and `..` records.
    pub fn read_dir(&self, dir: &IsoEntry) -> Result<Vec<IsoEntry>, IsoError> {
        if !dir.is_dir {
            return Err(IsoError::Corrupt("read_dir on a file"));
        }
        let mut data = vec![0u8; dir.len as usize];
        self.source
            .read_at(dir.lba as u64 * SECTOR_LEN as u64, &mut data)?;

        let mut entries = Vec::new();
        let mut offset = 0usize;
        while offset < data.len() {
            let record_len = data[offset] as usize;
            if record_len == 0 {
                // A zero length pads out the rest of the sector; the next record,
                // if any, starts at the following sector boundary.
                let next = (offset / SECTOR_LEN + 1) * SECTOR_LEN;
                if next >= data.len() {
                    break;
                }
                offset = next;
                continue;
            }
            if offset + record_len > data.len() {
                return Err(IsoError::Corrupt("directory record overruns extent"));
            }
            if let Some(entry) = parse_dir_record(&data[offset..offset + record_len]) {
                // "." and ".." carry single-byte identifiers 0x00 and 0x01.
                if !entry.name.is_empty() && entry.name != "\u{1}" {
                    entries.push(entry);
                }
            }
            offset += record_len;
        }
        Ok(entries)
    }

    /// Resolves a `/`-separated path, matching names case-insensitively.
    pub fn find(&self, path: &str) -> Result<Option<IsoEntry>, IsoError> {
        let mut current = self.root.clone();
        for component in path.split('/').filter(|c| !c.is_empty()) {
            if !current.is_dir {
                return Ok(None);
            }
            let entries = self.read_dir(&current)?;
            match entries
                .into_iter()
                .find(|e| e.name.eq_ignore_ascii_case(component))
            {
                Some(found) => current = found,
                None => return Ok(None),
            }
        }
        Ok(Some(current))
    }

    /// Reads a file's contents, or `None` if the path is absent or is a directory.
    pub fn read_file(&self, path: &str) -> Result<Option<Vec<u8>>, IsoError> {
        let Some(entry) = self.find(path)? else {
            return Ok(None);
        };
        if entry.is_dir {
            return Ok(None);
        }
        let mut data = vec![0u8; entry.len as usize];
        self.source
            .read_at(entry.lba as u64 * SECTOR_LEN as u64, &mut data)?;
        Ok(Some(data))
    }
}

fn parse_dir_record(record: &[u8]) -> Option<IsoEntry> {
    if record.len() < 33 {
        return None;
    }
    let lba = u32::from_le_bytes(record[2..6].try_into().ok()?);
    let len = u32::from_le_bytes(record[10..14].try_into().ok()?);
    let flags = record[25];
    let id_len = record[32] as usize;
    let id = record.get(33..33 + id_len)?;

    let name = if id == [0] {
        String::new() // "." — the record for the directory itself
    } else if id == [1] {
        "\u{1}".to_string() // ".."
    } else {
        // Strip the ";1" version suffix ISO9660 appends to file identifiers.
        let text = String::from_utf8_lossy(id);
        text.split(';').next().unwrap_or(&text).to_string()
    };

    Some(IsoEntry {
        name,
        lba,
        len,
        is_dir: flags & FLAG_DIRECTORY != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{tiny_png, IsoBuilder, SfoBuilder};

    fn image() -> Vec<u8> {
        IsoBuilder::new()
            .volume_id("DAXTER")
            .param_sfo(SfoBuilder::new().text("TITLE", "Daxter").build())
            .icon0(tiny_png(144, 80))
            .build()
    }

    #[test]
    fn reads_the_volume_id() {
        let iso = IsoReader::new(image()).unwrap();
        assert_eq!(iso.volume_id(), "DAXTER");
    }

    #[test]
    fn finds_nested_files_and_strips_version_suffixes() {
        let iso = IsoReader::new(image()).unwrap();
        let entry = iso
            .find("PSP_GAME/PARAM.SFO")
            .unwrap()
            .expect("should exist");
        assert_eq!(entry.name, "PARAM.SFO");
        assert!(!entry.is_dir);
    }

    #[test]
    fn path_matching_is_case_insensitive() {
        let iso = IsoReader::new(image()).unwrap();
        assert!(iso.find("psp_game/param.sfo").unwrap().is_some());
    }

    #[test]
    fn reads_file_contents_back_intact() {
        let icon = tiny_png(144, 80);
        let bytes = IsoBuilder::new()
            .param_sfo(SfoBuilder::new().text("TITLE", "Daxter").build())
            .icon0(icon.clone())
            .build();
        let iso = IsoReader::new(bytes).unwrap();
        let read = iso.read_file("PSP_GAME/ICON0.PNG").unwrap().unwrap();
        assert_eq!(read, icon);
    }

    #[test]
    fn walks_into_a_second_level_directory() {
        let iso = IsoReader::new(image()).unwrap();
        assert!(iso.find("PSP_GAME/SYSDIR/EBOOT.BIN").unwrap().is_some());
    }

    #[test]
    fn read_dir_hides_dot_and_dotdot() {
        let iso = IsoReader::new(image()).unwrap();
        let psp_game = iso.find("PSP_GAME").unwrap().unwrap();
        let names: Vec<_> = iso
            .read_dir(&psp_game)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, vec!["PARAM.SFO", "ICON0.PNG", "SYSDIR"]);
    }

    #[test]
    fn missing_paths_are_none_not_errors() {
        let iso = IsoReader::new(image()).unwrap();
        assert_eq!(iso.find("PSP_GAME/NOPE.BIN").unwrap(), None);
        assert_eq!(iso.read_file("PSP_GAME/NOPE.BIN").unwrap(), None);
    }

    #[test]
    fn reading_a_directory_as_a_file_yields_none() {
        let iso = IsoReader::new(image()).unwrap();
        assert_eq!(iso.read_file("PSP_GAME").unwrap(), None);
    }

    #[test]
    fn rejects_data_that_is_not_an_iso() {
        let not_an_iso = vec![0u8; SECTOR_LEN * 20];
        assert!(matches!(
            IsoReader::new(not_an_iso),
            Err(IsoError::NotIso9660)
        ));
    }

    #[test]
    fn rejects_an_image_too_short_to_hold_a_pvd() {
        assert!(matches!(
            IsoReader::new(vec![0u8; 100]),
            Err(IsoError::NotIso9660)
        ));
    }
}
