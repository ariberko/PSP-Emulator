//! PBP container reader.
//!
//! PBP ("PlayStation Boot Package") wraps a homebrew app or a PSN download into
//! one file. The header is eight absolute offsets; a section's length is the
//! distance to the next offset, so an empty section is one whose offset equals
//! its successor's.
//!
//! ```text
//! 0x00  u32  magic = "\0PBP"
//! 0x04  u32  version
//! 0x08  u32  offset PARAM.SFO
//! 0x0C  u32  offset ICON0.PNG   ← XMB list icon
//! 0x10  u32  offset ICON1.PMF   ← animated icon
//! 0x14  u32  offset PIC0.PNG
//! 0x18  u32  offset PIC1.PNG    ← XMB background
//! 0x1C  u32  offset SND0.AT3
//! 0x20  u32  offset DATA.PSP
//! 0x24  u32  offset DATA.PSAR
//! ```

use std::fmt;

/// `"\0PBP"` read as a little-endian u32.
const PBP_MAGIC: u32 = 0x50425000;
const HEADER_LEN: usize = 0x28;
const SECTION_COUNT: usize = 8;

/// Sections in header order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PbpSection {
    ParamSfo = 0,
    Icon0Png = 1,
    Icon1Pmf = 2,
    Pic0Png = 3,
    Pic1Png = 4,
    Snd0At3 = 5,
    DataPsp = 6,
    DataPsar = 7,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PbpError {
    BadMagic,
    Truncated,
    /// Offsets must not run backwards or past the end of the file.
    BadOffsets,
}

impl fmt::Display for PbpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PbpError::BadMagic => write!(f, "not a PBP file (bad magic)"),
            PbpError::Truncated => write!(f, "PBP truncated before the end of its header"),
            PbpError::BadOffsets => {
                write!(f, "PBP section offsets are not monotonic or run past EOF")
            }
        }
    }
}

impl std::error::Error for PbpError {}

/// A PBP borrowed from an in-memory buffer.
#[derive(Debug)]
pub struct Pbp<'a> {
    data: &'a [u8],
    offsets: [u32; SECTION_COUNT],
}

impl<'a> Pbp<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, PbpError> {
        if data.len() < HEADER_LEN {
            return Err(PbpError::Truncated);
        }
        if u32::from_le_bytes(data[0..4].try_into().unwrap()) != PBP_MAGIC {
            return Err(PbpError::BadMagic);
        }

        let mut offsets = [0u32; SECTION_COUNT];
        for (i, slot) in offsets.iter_mut().enumerate() {
            let at = 0x08 + i * 4;
            *slot = u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
        }

        // Section lengths are derived by subtraction, so a non-monotonic or
        // out-of-range table would produce bogus slices rather than a clean error.
        let mut previous = HEADER_LEN as u32;
        for offset in offsets {
            if offset < previous || offset as usize > data.len() {
                return Err(PbpError::BadOffsets);
            }
            previous = offset;
        }

        Ok(Self { data, offsets })
    }

    /// Returns a section's bytes, or `None` when the section is absent (zero length).
    pub fn section(&self, section: PbpSection) -> Option<&'a [u8]> {
        let index = section as usize;
        let start = self.offsets[index] as usize;
        // The last section runs to the end of the file.
        let end = match self.offsets.get(index + 1) {
            Some(next) => *next as usize,
            None => self.data.len(),
        };
        if end <= start {
            return None;
        }
        self.data.get(start..end)
    }

    pub fn param_sfo(&self) -> Option<&'a [u8]> {
        self.section(PbpSection::ParamSfo)
    }

    /// The 144×80 icon the XMB shows in its game list.
    pub fn icon0(&self) -> Option<&'a [u8]> {
        self.section(PbpSection::Icon0Png)
    }

    /// The 480×272 background the XMB shows when the entry is selected.
    pub fn pic1(&self) -> Option<&'a [u8]> {
        self.section(PbpSection::Pic1Png)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sfo::Sfo;
    use crate::testkit::{tiny_png, PbpBuilder, SfoBuilder};

    #[test]
    fn extracts_sfo_icon_and_background() {
        let sfo = SfoBuilder::new().text("TITLE", "Cave Story").build();
        let icon = tiny_png(144, 80);
        let background = tiny_png(480, 272);
        let bytes = PbpBuilder::new()
            .param_sfo(sfo.clone())
            .icon0(icon.clone())
            .pic1(background.clone())
            .data_psp(b"payload".to_vec())
            .build();

        let pbp = Pbp::parse(&bytes).unwrap();
        assert_eq!(pbp.param_sfo(), Some(sfo.as_slice()));
        assert_eq!(pbp.icon0(), Some(icon.as_slice()));
        assert_eq!(pbp.pic1(), Some(background.as_slice()));
        assert_eq!(
            Sfo::parse(pbp.param_sfo().unwrap()).unwrap().title(),
            Some("Cave Story")
        );
    }

    #[test]
    fn absent_sections_are_none_rather_than_empty_slices() {
        let bytes = PbpBuilder::new()
            .param_sfo(SfoBuilder::new().text("TITLE", "x").build())
            .build();
        let pbp = Pbp::parse(&bytes).unwrap();
        assert!(pbp.param_sfo().is_some());
        assert_eq!(pbp.icon0(), None);
        assert_eq!(pbp.pic1(), None);
        assert_eq!(pbp.section(PbpSection::Snd0At3), None);
    }

    #[test]
    fn reads_the_trailing_section_to_end_of_file() {
        let bytes = PbpBuilder::new().data_psp(b"the tail".to_vec()).build();
        let pbp = Pbp::parse(&bytes).unwrap();
        assert_eq!(pbp.section(PbpSection::DataPsp), Some(&b"the tail"[..]));
    }

    #[test]
    fn rejects_non_pbp_data() {
        assert_eq!(Pbp::parse(&[0u8; 64]).unwrap_err(), PbpError::BadMagic);
    }

    #[test]
    fn rejects_a_short_header() {
        assert_eq!(
            Pbp::parse(&[0x00, b'P', b'B', b'P']).unwrap_err(),
            PbpError::Truncated
        );
    }

    #[test]
    fn rejects_offsets_running_past_the_end() {
        let mut bytes = PbpBuilder::new().icon0(tiny_png(1, 1)).build();
        bytes[0x0C..0x10].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        assert_eq!(Pbp::parse(&bytes).unwrap_err(), PbpError::BadOffsets);
    }

    #[test]
    fn rejects_offsets_running_backwards() {
        let mut bytes = PbpBuilder::new()
            .param_sfo(SfoBuilder::new().text("TITLE", "x").build())
            .icon0(tiny_png(1, 1))
            .build();
        // Point ICON0 before PARAM.SFO.
        bytes[0x0C..0x10].copy_from_slice(&0x20u32.to_le_bytes());
        assert_eq!(Pbp::parse(&bytes).unwrap_err(), PbpError::BadOffsets);
    }
}
