//! PARAM.SFO parser.
//!
//! Every PSP title carries a PARAM.SFO: a small key/value table holding the
//! display title, disc ID, category and firmware requirement. It is the only
//! place a game's human-readable name is stored, so the XMB game list is built
//! entirely out of these.
//!
//! Layout is a 20-byte header, a fixed-stride index table, then separate key
//! and data regions that the index points into:
//!
//! ```text
//! 0x00  u32  magic = "\0PSF"
//! 0x04  u32  version (0x00000101)
//! 0x08  u32  key_table_start    ─┐ both absolute from the start of the file
//! 0x0C  u32  data_table_start   ─┘
//! 0x10  u32  index_table_entries
//! 0x14  [IndexEntry; index_table_entries]
//! ```

use std::collections::BTreeMap;
use std::fmt;

/// `"\0PSF"` read as a little-endian u32.
const SFO_MAGIC: u32 = 0x46535000;
const HEADER_LEN: usize = 0x14;
const INDEX_ENTRY_LEN: usize = 0x10;

/// Data format tags used by the index table.
const FMT_UTF8_NOT_TERMINATED: u16 = 0x0004;
const FMT_UTF8: u16 = 0x0204;
const FMT_U32: u16 = 0x0404;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SfoValue {
    Text(String),
    Int(u32),
    /// A format tag we do not model, kept as raw bytes so nothing is silently lost.
    Raw(Vec<u8>),
}

impl SfoValue {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            SfoValue::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<u32> {
        match self {
            SfoValue::Int(v) => Some(*v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SfoError {
    /// Not a PARAM.SFO at all.
    BadMagic,
    /// The file ends before a structure it declares.
    Truncated { what: &'static str, offset: usize },
    /// An index entry points outside the file.
    BadOffset { what: &'static str, offset: usize },
}

impl fmt::Display for SfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SfoError::BadMagic => write!(f, "not a PARAM.SFO (bad magic)"),
            SfoError::Truncated { what, offset } => {
                write!(
                    f,
                    "PARAM.SFO truncated reading {what} at offset {offset:#x}"
                )
            }
            SfoError::BadOffset { what, offset } => {
                write!(f, "PARAM.SFO {what} offset {offset:#x} is out of bounds")
            }
        }
    }
}

impl std::error::Error for SfoError {}

/// A parsed PARAM.SFO.
///
/// Keys are kept in a `BTreeMap` so iteration order is stable, which keeps
/// snapshot-style output deterministic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sfo {
    entries: BTreeMap<String, SfoValue>,
}

impl Sfo {
    pub fn parse(data: &[u8]) -> Result<Self, SfoError> {
        if data.len() < HEADER_LEN {
            return Err(SfoError::Truncated {
                what: "header",
                offset: 0,
            });
        }
        if u32(data, 0) != SFO_MAGIC {
            return Err(SfoError::BadMagic);
        }

        let key_table_start = u32(data, 0x08) as usize;
        let data_table_start = u32(data, 0x0C) as usize;
        let count = u32(data, 0x10) as usize;

        // Reject an entry count the file cannot possibly hold before allocating
        // anything sized from it.
        let index_len = count
            .checked_mul(INDEX_ENTRY_LEN)
            .ok_or(SfoError::Truncated {
                what: "index table",
                offset: HEADER_LEN,
            })?;
        if HEADER_LEN + index_len > data.len() {
            return Err(SfoError::Truncated {
                what: "index table",
                offset: HEADER_LEN,
            });
        }

        let mut entries = BTreeMap::new();
        for i in 0..count {
            let base = HEADER_LEN + i * INDEX_ENTRY_LEN;
            let key_offset = u16(data, base) as usize;
            let data_fmt = u16(data, base + 0x02);
            let data_len = u32(data, base + 0x04) as usize;
            let data_offset = u32(data, base + 0x0C) as usize;

            let key_at = key_table_start
                .checked_add(key_offset)
                .ok_or(SfoError::BadOffset {
                    what: "key",
                    offset: key_offset,
                })?;
            let key = read_cstr(data, key_at).ok_or(SfoError::BadOffset {
                what: "key",
                offset: key_at,
            })?;

            let value_at =
                data_table_start
                    .checked_add(data_offset)
                    .ok_or(SfoError::BadOffset {
                        what: "value",
                        offset: data_offset,
                    })?;
            let end = value_at.checked_add(data_len).ok_or(SfoError::BadOffset {
                what: "value",
                offset: value_at,
            })?;
            if end > data.len() {
                return Err(SfoError::BadOffset {
                    what: "value",
                    offset: value_at,
                });
            }
            let raw = &data[value_at..end];

            let value = match data_fmt {
                FMT_UTF8 | FMT_UTF8_NOT_TERMINATED => {
                    // Declared length includes the NUL for 0x0204 and any
                    // padding the writer left behind; trim before decoding.
                    let text = raw.split(|&b| b == 0).next().unwrap_or(raw);
                    SfoValue::Text(String::from_utf8_lossy(text).into_owned())
                }
                FMT_U32 if raw.len() >= 4 => SfoValue::Int(u32(raw, 0)),
                _ => SfoValue::Raw(raw.to_vec()),
            };
            entries.insert(key, value);
        }

        Ok(Sfo { entries })
    }

    pub fn get(&self, key: &str) -> Option<&SfoValue> {
        self.entries.get(key)
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(SfoValue::as_text)
    }

    pub fn int(&self, key: &str) -> Option<u32> {
        self.get(key).and_then(SfoValue::as_int)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &SfoValue)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Display title, e.g. `"Daxter"`.
    pub fn title(&self) -> Option<&str> {
        self.text("TITLE")
    }

    /// Disc ID, e.g. `"UCUS98618"`. Also the key PPSSPP uses to name save data.
    pub fn disc_id(&self) -> Option<&str> {
        self.text("DISC_ID")
    }

    /// `CATEGORY` distinguishes a game (`"UG"`/`"MG"`) from a save (`"MS"`),
    /// a theme (`"PP"`), an update, and so on.
    pub fn category(&self) -> Option<&str> {
        self.text("CATEGORY")
    }

    pub fn disc_version(&self) -> Option<&str> {
        self.text("DISC_VERSION")
    }

    /// Minimum firmware, e.g. `"6.60"`.
    pub fn system_version(&self) -> Option<&str> {
        self.text("PSP_SYSTEM_VER")
    }

    pub fn parental_level(&self) -> Option<u32> {
        self.int("PARENTAL_LEVEL")
    }
}

fn u16(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

fn u32(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

fn read_cstr(data: &[u8], at: usize) -> Option<String> {
    let rest = data.get(at..)?;
    let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
    Some(String::from_utf8_lossy(&rest[..end]).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::SfoBuilder;

    #[test]
    fn parses_a_typical_game_sfo() {
        let bytes = SfoBuilder::new()
            .text("CATEGORY", "UG")
            .text("DISC_ID", "UCUS98618")
            .text("DISC_VERSION", "1.00")
            .int("PARENTAL_LEVEL", 5)
            .text("PSP_SYSTEM_VER", "6.60")
            .text("TITLE", "Daxter")
            .build();

        let sfo = Sfo::parse(&bytes).expect("should parse");
        assert_eq!(sfo.title(), Some("Daxter"));
        assert_eq!(sfo.disc_id(), Some("UCUS98618"));
        assert_eq!(sfo.category(), Some("UG"));
        assert_eq!(sfo.disc_version(), Some("1.00"));
        assert_eq!(sfo.system_version(), Some("6.60"));
        assert_eq!(sfo.parental_level(), Some(5));
    }

    #[test]
    fn strips_trailing_nul_and_padding_from_strings() {
        // Real writers pad a value out to its allocated size; the declared
        // length covers the padding, so a naive decode keeps the NULs.
        let bytes = SfoBuilder::new()
            .padded_text("TITLE", "Wipeout Pure", 32)
            .build();
        let sfo = Sfo::parse(&bytes).unwrap();
        assert_eq!(sfo.title(), Some("Wipeout Pure"));
    }

    #[test]
    fn keeps_utf8_titles_intact() {
        let bytes = SfoBuilder::new().text("TITLE", "パタポン").build();
        let sfo = Sfo::parse(&bytes).unwrap();
        assert_eq!(sfo.title(), Some("パタポン"));
    }

    #[test]
    fn rejects_non_sfo_data() {
        let junk = b"This is not a PARAM.SFO at all, just some bytes.";
        assert_eq!(Sfo::parse(junk), Err(SfoError::BadMagic));
    }

    #[test]
    fn rejects_short_header() {
        assert!(matches!(
            Sfo::parse(&[0x00, 0x50, 0x53, 0x46]),
            Err(SfoError::Truncated { .. })
        ));
    }

    #[test]
    fn rejects_an_entry_count_the_file_cannot_hold() {
        // A corrupt or hostile file claiming 100k entries must not cause a huge
        // allocation or an out-of-bounds read.
        let mut bytes = SfoBuilder::new().text("TITLE", "x").build();
        bytes[0x10..0x14].copy_from_slice(&100_000u32.to_le_bytes());
        assert!(matches!(
            Sfo::parse(&bytes),
            Err(SfoError::Truncated {
                what: "index table",
                ..
            })
        ));
    }

    #[test]
    fn rejects_a_value_pointing_past_the_end() {
        let mut bytes = SfoBuilder::new().text("TITLE", "x").build();
        // Push the first index entry's data offset far beyond the file.
        let data_offset_field = 0x14 + 0x0C;
        bytes[data_offset_field..data_offset_field + 4]
            .copy_from_slice(&0xFFFF_0000u32.to_le_bytes());
        assert!(matches!(
            Sfo::parse(&bytes),
            Err(SfoError::BadOffset { .. })
        ));
    }

    #[test]
    fn an_empty_table_is_valid_but_yields_nothing() {
        let sfo = Sfo::parse(&SfoBuilder::new().build()).unwrap();
        assert!(sfo.is_empty());
        assert_eq!(sfo.title(), None);
    }
}
