//! CSO (CISO) reader — the compressed disc format most PSP libraries are stored in.
//!
//! A CSO is an ISO chopped into fixed-size blocks, each either raw-deflated or
//! stored verbatim, with an index giving each block's file offset. Presenting it as
//! a [`ReadAt`] means [`IsoReader`](crate::iso::IsoReader) walks a `.cso` and a
//! `.iso` through exactly the same code path.
//!
//! ```text
//! 0x00  u32  magic = "CISO"
//! 0x04  u32  header size (0x18)
//! 0x08  u64  uncompressed total size
//! 0x10  u32  block size (commonly 2048)
//! 0x14  u8   version
//! 0x15  u8   index shift — offsets are stored >> this
//! 0x16  u16  unused
//! 0x18  [u32; block_count + 1]  index
//! ```
//!
//! Each index entry's top bit marks a stored (uncompressed) block; the low 31
//! bits are the offset, shifted left by the index shift. The extra trailing
//! entry exists so every block's compressed length is `next - current`.

use std::cell::RefCell;
use std::io;

use crate::iso::ReadAt;

const CSO_MAGIC: &[u8; 4] = b"CISO";
const HEADER_LEN: usize = 0x18;
/// Top bit of an index entry: block is stored, not deflated.
const STORED_FLAG: u32 = 0x8000_0000;
const OFFSET_MASK: u32 = 0x7FFF_FFFF;

pub struct CsoReader<S: ReadAt> {
    source: S,
    total_size: u64,
    block_size: u32,
    index: Vec<u32>,
    index_shift: u8,
    /// Single-block cache. Directory walking re-reads the same sector many times
    /// while resolving a path, and inflating a block per read is wasteful.
    cache: RefCell<Option<(usize, Vec<u8>)>>,
}

#[derive(Debug)]
pub enum CsoError {
    NotCso,
    Unsupported(&'static str),
    Corrupt(&'static str),
    Io(io::Error),
}

impl std::fmt::Display for CsoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CsoError::NotCso => write!(f, "not a CSO image"),
            CsoError::Unsupported(what) => write!(f, "unsupported CSO: {what}"),
            CsoError::Corrupt(what) => write!(f, "corrupt CSO: {what}"),
            CsoError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CsoError {}

impl From<io::Error> for CsoError {
    fn from(e: io::Error) -> Self {
        CsoError::Io(e)
    }
}

impl<S: ReadAt> CsoReader<S> {
    pub fn new(source: S) -> Result<Self, CsoError> {
        let mut header = [0u8; HEADER_LEN];
        source
            .read_at(0, &mut header)
            .map_err(|_| CsoError::NotCso)?;
        if &header[0..4] != CSO_MAGIC {
            return Err(CsoError::NotCso);
        }

        let total_size = u64::from_le_bytes(header[8..16].try_into().unwrap());
        let block_size = u32::from_le_bytes(header[16..20].try_into().unwrap());
        let index_shift = header[21];

        if block_size == 0 {
            return Err(CsoError::Corrupt("block size is zero"));
        }
        if index_shift > 31 {
            return Err(CsoError::Unsupported("index shift out of range"));
        }

        let block_count = total_size.div_ceil(block_size as u64) as usize;
        // Guard the allocation: a corrupt total_size must not size a Vec.
        let index_bytes = (block_count + 1) * 4;
        if HEADER_LEN as u64 + index_bytes as u64 > source.size() {
            return Err(CsoError::Corrupt("index does not fit in the file"));
        }

        let mut raw = vec![0u8; index_bytes];
        source.read_at(HEADER_LEN as u64, &mut raw)?;
        let index = raw
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();

        Ok(Self {
            source,
            total_size,
            block_size,
            index,
            index_shift,
            cache: RefCell::new(None),
        })
    }

    /// Uncompressed size of the underlying ISO.
    pub fn uncompressed_size(&self) -> u64 {
        self.total_size
    }

    pub fn block_size(&self) -> u32 {
        self.block_size
    }

    fn block(&self, index: usize) -> Result<Vec<u8>, io::Error> {
        if let Some((cached, bytes)) = self.cache.borrow().as_ref() {
            if *cached == index {
                return Ok(bytes.clone());
            }
        }

        let entry = *self
            .index
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "block past end"))?;
        let next = *self.index.get(index + 1).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "missing trailing index entry")
        })?;

        let stored = entry & STORED_FLAG != 0;
        let start = ((entry & OFFSET_MASK) as u64) << self.index_shift;
        let end = ((next & OFFSET_MASK) as u64) << self.index_shift;
        if end <= start {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "non-monotonic CSO index",
            ));
        }

        let mut packed = vec![0u8; (end - start) as usize];
        self.source.read_at(start, &mut packed)?;

        let plain = if stored {
            packed
        } else {
            miniz_oxide::inflate::decompress_to_vec(&packed).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("CSO block {index} failed to inflate: {e:?}"),
                )
            })?
        };

        *self.cache.borrow_mut() = Some((index, plain.clone()));
        Ok(plain)
    }
}

impl<S: ReadAt> ReadAt for CsoReader<S> {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        if offset + buf.len() as u64 > self.total_size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "read past end of CSO",
            ));
        }

        let mut written = 0usize;
        let mut position = offset;
        while written < buf.len() {
            let index = (position / self.block_size as u64) as usize;
            let within = (position % self.block_size as u64) as usize;
            let block = self.block(index)?;
            let available = block
                .len()
                .checked_sub(within)
                .filter(|n| *n > 0)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "short CSO block"))?;
            let take = available.min(buf.len() - written);
            buf[written..written + take].copy_from_slice(&block[within..within + take]);
            written += take;
            position += take as u64;
        }
        Ok(())
    }

    fn size(&self) -> u64 {
        self.total_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iso::IsoReader;
    use crate::testkit::{cso_store_only, tiny_png, IsoBuilder, SfoBuilder};
    use miniz_oxide::deflate::compress_to_vec;

    fn iso_image() -> Vec<u8> {
        IsoBuilder::new()
            .volume_id("PATAPON")
            .param_sfo(SfoBuilder::new().text("TITLE", "Patapon").build())
            .icon0(tiny_png(144, 80))
            .build()
    }

    /// Deflates every block, which is what a real `maxcso` output looks like.
    fn cso_deflated(iso: &[u8], block_size: u32) -> Vec<u8> {
        let block_count = iso.len().div_ceil(block_size as usize);
        let header_len = HEADER_LEN;
        let index_len = (block_count + 1) * 4;

        let mut out = vec![0u8; header_len + index_len];
        out[0..4].copy_from_slice(CSO_MAGIC);
        out[4..8].copy_from_slice(&(header_len as u32).to_le_bytes());
        out[8..16].copy_from_slice(&(iso.len() as u64).to_le_bytes());
        out[16..20].copy_from_slice(&block_size.to_le_bytes());
        out[20] = 1;
        out[21] = 0;

        for i in 0..block_count {
            let start = i * block_size as usize;
            let end = ((i + 1) * block_size as usize).min(iso.len());
            let deflated = compress_to_vec(&iso[start..end], 6);
            let offset = out.len() as u32;
            out[header_len + i * 4..][..4].copy_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(&deflated);
        }
        let end_entry = out.len() as u32;
        out[header_len + block_count * 4..][..4].copy_from_slice(&end_entry.to_le_bytes());
        out
    }

    #[test]
    fn stored_blocks_round_trip_the_whole_image() {
        let iso = iso_image();
        let cso = CsoReader::new(cso_store_only(&iso, 2048)).unwrap();
        assert_eq!(cso.uncompressed_size(), iso.len() as u64);

        let mut out = vec![0u8; iso.len()];
        cso.read_at(0, &mut out).unwrap();
        assert_eq!(out, iso);
    }

    #[test]
    fn deflated_blocks_round_trip_the_whole_image() {
        let iso = iso_image();
        let cso = CsoReader::new(cso_deflated(&iso, 2048)).unwrap();
        let mut out = vec![0u8; iso.len()];
        cso.read_at(0, &mut out).unwrap();
        assert_eq!(out, iso);
    }

    #[test]
    fn the_iso_walker_reads_straight_through_a_cso() {
        // The point of the ReadAt abstraction: no CSO-specific code in the walker.
        let cso = CsoReader::new(cso_deflated(&iso_image(), 2048)).unwrap();
        let iso = IsoReader::new(cso).unwrap();
        assert_eq!(iso.volume_id(), "PATAPON");
        let sfo = iso.read_file("PSP_GAME/PARAM.SFO").unwrap().unwrap();
        assert_eq!(
            crate::sfo::Sfo::parse(&sfo).unwrap().title(),
            Some("Patapon")
        );
    }

    #[test]
    fn reads_spanning_a_block_boundary_are_stitched() {
        let iso = iso_image();
        // A block size smaller than a sector forces every sector read to span blocks.
        let cso = CsoReader::new(cso_deflated(&iso, 512)).unwrap();
        let mut out = vec![0u8; 2048];
        cso.read_at(700, &mut out).unwrap();
        assert_eq!(out, &iso[700..700 + 2048]);
    }

    #[test]
    fn unaligned_reads_land_on_the_right_bytes() {
        let iso = iso_image();
        let cso = CsoReader::new(cso_store_only(&iso, 2048)).unwrap();
        for offset in [1u64, 17, 2047, 2049, 4095] {
            let mut out = vec![0u8; 33];
            cso.read_at(offset, &mut out).unwrap();
            assert_eq!(
                out,
                &iso[offset as usize..offset as usize + 33],
                "at {offset}"
            );
        }
    }

    #[test]
    fn rejects_data_that_is_not_a_cso() {
        assert!(matches!(
            CsoReader::new(vec![0u8; 4096]),
            Err(CsoError::NotCso)
        ));
    }

    #[test]
    fn rejects_an_index_that_cannot_fit_in_the_file() {
        let mut cso = cso_store_only(&iso_image(), 2048);
        // Claim a huge uncompressed size, implying an index far bigger than the file.
        cso[8..16].copy_from_slice(&(1u64 << 40).to_le_bytes());
        assert!(matches!(CsoReader::new(cso), Err(CsoError::Corrupt(_))));
    }

    #[test]
    fn reading_past_the_end_errors() {
        let iso = iso_image();
        let cso = CsoReader::new(cso_store_only(&iso, 2048)).unwrap();
        let mut out = vec![0u8; 16];
        assert!(cso.read_at(iso.len() as u64 - 8, &mut out).is_err());
    }
}
