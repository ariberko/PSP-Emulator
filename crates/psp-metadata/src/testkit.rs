//! Builders that synthesise the binary formats this crate reads.
//!
//! Tests need real PARAM.SFO / PBP / ISO bytes, and shipping copyrighted game
//! files is out of the question. These builders emit the same structures a PSP
//! toolchain would, so the parsers are exercised against genuine layouts rather
//! than against hand-tweaked blobs that happen to match the parser's mistakes.
//!
//! Available in test builds, and under the `testkit` feature for downstream use.

use crate::iso::SECTOR_LEN;

const SFO_HEADER_LEN: usize = 0x14;
const SFO_INDEX_ENTRY_LEN: usize = 0x10;

enum SfoField {
    Text {
        key: String,
        value: String,
        max: Option<usize>,
    },
    Int {
        key: String,
        value: u32,
    },
}

/// Builds a valid PARAM.SFO.
#[derive(Default)]
pub struct SfoBuilder {
    fields: Vec<SfoField>,
}

impl SfoBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(mut self, key: &str, value: &str) -> Self {
        self.fields.push(SfoField::Text {
            key: key.to_string(),
            value: value.to_string(),
            max: None,
        });
        self
    }

    /// A string padded out to `max` bytes, the way real writers reserve space.
    pub fn padded_text(mut self, key: &str, value: &str, max: usize) -> Self {
        self.fields.push(SfoField::Text {
            key: key.to_string(),
            value: value.to_string(),
            max: Some(max),
        });
        self
    }

    pub fn int(mut self, key: &str, value: u32) -> Self {
        self.fields.push(SfoField::Int {
            key: key.to_string(),
            value,
        });
        self
    }

    pub fn build(self) -> Vec<u8> {
        // The index table is sorted by key in real files; matching that keeps
        // fixtures faithful.
        let mut fields = self.fields;
        fields.sort_by_key(|f| match f {
            SfoField::Text { key, .. } | SfoField::Int { key, .. } => key.clone(),
        });

        let mut keys = Vec::new();
        let mut values = Vec::new();
        // (key_offset, fmt, data_len, data_max, data_offset)
        let mut index: Vec<(u16, u16, u32, u32, u32)> = Vec::new();

        for field in &fields {
            let key_offset = keys.len() as u16;
            match field {
                SfoField::Text { key, value, max } => {
                    keys.extend_from_slice(key.as_bytes());
                    keys.push(0);

                    let data_offset = values.len() as u32;
                    let bytes = value.as_bytes();
                    // Declared length includes the terminator.
                    let data_len = bytes.len() as u32 + 1;
                    let data_max = match max {
                        Some(m) => (*m).max(data_len as usize) as u32,
                        None => align4(data_len as usize) as u32,
                    };
                    values.extend_from_slice(bytes);
                    values.resize(values.len() + (data_max as usize - bytes.len()), 0);
                    index.push((key_offset, 0x0204, data_len, data_max, data_offset));
                }
                SfoField::Int { key, value } => {
                    keys.extend_from_slice(key.as_bytes());
                    keys.push(0);

                    let data_offset = values.len() as u32;
                    values.extend_from_slice(&value.to_le_bytes());
                    index.push((key_offset, 0x0404, 4, 4, data_offset));
                }
            }
        }

        // Real files pad the key table so the data table starts 4-byte aligned.
        let key_table_pad = align4(keys.len()) - keys.len();
        keys.resize(keys.len() + key_table_pad, 0);

        let key_table_start = SFO_HEADER_LEN + index.len() * SFO_INDEX_ENTRY_LEN;
        let data_table_start = key_table_start + keys.len();

        let mut out = Vec::with_capacity(data_table_start + values.len());
        out.extend_from_slice(&0x46535000u32.to_le_bytes()); // "\0PSF"
        out.extend_from_slice(&0x0000_0101u32.to_le_bytes()); // version 1.1
        out.extend_from_slice(&(key_table_start as u32).to_le_bytes());
        out.extend_from_slice(&(data_table_start as u32).to_le_bytes());
        out.extend_from_slice(&(index.len() as u32).to_le_bytes());

        for (key_offset, fmt, data_len, data_max, data_offset) in index {
            out.extend_from_slice(&key_offset.to_le_bytes());
            out.extend_from_slice(&fmt.to_le_bytes());
            out.extend_from_slice(&data_len.to_le_bytes());
            out.extend_from_slice(&data_max.to_le_bytes());
            out.extend_from_slice(&data_offset.to_le_bytes());
        }
        out.extend_from_slice(&keys);
        out.extend_from_slice(&values);
        out
    }
}

/// A minimal PNG. Not decoded by this crate — it is passed through to the UI —
/// so a valid signature plus an IHDR is enough to prove extraction is correct.
pub fn tiny_png(width: u32, height: u32) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(b"IHDR");
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    out.extend_from_slice(&(13u32).to_be_bytes());
    out.extend_from_slice(&ihdr);
    out.extend_from_slice(&0u32.to_be_bytes()); // CRC placeholder
    out
}

/// Builds a PBP container, the format used by PSN downloads and homebrew.
#[derive(Default)]
pub struct PbpBuilder {
    param_sfo: Vec<u8>,
    icon0: Vec<u8>,
    pic1: Vec<u8>,
    data_psp: Vec<u8>,
}

impl PbpBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn param_sfo(mut self, bytes: Vec<u8>) -> Self {
        self.param_sfo = bytes;
        self
    }

    pub fn icon0(mut self, bytes: Vec<u8>) -> Self {
        self.icon0 = bytes;
        self
    }

    pub fn pic1(mut self, bytes: Vec<u8>) -> Self {
        self.pic1 = bytes;
        self
    }

    pub fn data_psp(mut self, bytes: Vec<u8>) -> Self {
        self.data_psp = bytes;
        self
    }

    pub fn build(self) -> Vec<u8> {
        // Eight section offsets follow the magic and version.
        let header_len: u32 = 0x28;
        let sections = [
            self.param_sfo.as_slice(),
            self.icon0.as_slice(),
            &[], // ICON1.PMF
            &[], // PIC0.PNG
            self.pic1.as_slice(),
            &[], // SND0.AT3
            self.data_psp.as_slice(),
            &[], // DATA.PSAR
        ];

        let mut offsets = Vec::with_capacity(8);
        let mut cursor = header_len;
        for section in &sections {
            offsets.push(cursor);
            cursor += section.len() as u32;
        }

        let mut out = Vec::with_capacity(cursor as usize);
        out.extend_from_slice(&[0x00, b'P', b'B', b'P']);
        out.extend_from_slice(&0x0001_0000u32.to_le_bytes());
        for offset in offsets {
            out.extend_from_slice(&offset.to_le_bytes());
        }
        for section in sections {
            out.extend_from_slice(section);
        }
        out
    }
}

/// Builds an ISO9660 image laid out like a UMD rip: a `PSP_GAME` directory
/// holding `PARAM.SFO` and `ICON0.PNG`, plus `SYSDIR/EBOOT.BIN`.
///
/// Path tables are zeroed. The reader walks directory records rather than path
/// tables, which is what real PSP images require anyway.
pub struct IsoBuilder {
    volume_id: String,
    param_sfo: Vec<u8>,
    icon0: Vec<u8>,
    eboot: Vec<u8>,
}

impl Default for IsoBuilder {
    fn default() -> Self {
        Self {
            volume_id: "PSP_GAME".to_string(),
            param_sfo: Vec::new(),
            icon0: Vec::new(),
            eboot: b"fake eboot".to_vec(),
        }
    }
}

struct DirEntry {
    name: &'static str,
    lba: u32,
    len: u32,
    is_dir: bool,
}

impl IsoBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn volume_id(mut self, id: &str) -> Self {
        self.volume_id = id.to_string();
        self
    }

    pub fn param_sfo(mut self, bytes: Vec<u8>) -> Self {
        self.param_sfo = bytes;
        self
    }

    pub fn icon0(mut self, bytes: Vec<u8>) -> Self {
        self.icon0 = bytes;
        self
    }

    pub fn build(self) -> Vec<u8> {
        // Fixed layout: 16 system sectors, PVD, terminator, then one sector per
        // directory followed by the file payloads.
        const PVD_LBA: u32 = 16;
        const ROOT_LBA: u32 = 18;
        const PSP_GAME_LBA: u32 = 19;
        const SYSDIR_LBA: u32 = 20;
        let sfo_lba: u32 = 21;
        let sfo_sectors = sectors_for(self.param_sfo.len());
        let icon_lba = sfo_lba + sfo_sectors;
        let icon_sectors = sectors_for(self.icon0.len());
        let eboot_lba = icon_lba + icon_sectors;
        let eboot_sectors = sectors_for(self.eboot.len());
        let total_sectors = eboot_lba + eboot_sectors;

        let mut image = vec![0u8; total_sectors as usize * SECTOR_LEN];

        // --- Primary Volume Descriptor -------------------------------------
        let pvd = &mut image[PVD_LBA as usize * SECTOR_LEN..][..SECTOR_LEN];
        pvd[0] = 1; // primary volume descriptor
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[6] = 1;
        pad_string(&mut pvd[8..40], "PSP GAME");
        pad_string(&mut pvd[40..72], &self.volume_id);
        write_both_endian_u32(&mut pvd[80..88], total_sectors);
        write_both_endian_u16(&mut pvd[120..124], 1); // volume set size
        write_both_endian_u16(&mut pvd[124..128], 1); // volume sequence number
        write_both_endian_u16(&mut pvd[128..132], SECTOR_LEN as u16);
        // Root directory record lives inside the PVD at offset 156.
        let root_record = dir_record(&DirEntry {
            name: "\0",
            lba: ROOT_LBA,
            len: SECTOR_LEN as u32,
            is_dir: true,
        });
        pvd[156..156 + root_record.len()].copy_from_slice(&root_record);

        // --- Volume descriptor set terminator ------------------------------
        let term = &mut image[(PVD_LBA as usize + 1) * SECTOR_LEN..][..SECTOR_LEN];
        term[0] = 0xFF;
        term[1..6].copy_from_slice(b"CD001");
        term[6] = 1;

        // --- Directories ---------------------------------------------------
        write_directory(
            &mut image,
            ROOT_LBA,
            ROOT_LBA,
            &[DirEntry {
                name: "PSP_GAME",
                lba: PSP_GAME_LBA,
                len: SECTOR_LEN as u32,
                is_dir: true,
            }],
        );
        write_directory(
            &mut image,
            PSP_GAME_LBA,
            ROOT_LBA,
            &[
                DirEntry {
                    name: "PARAM.SFO;1",
                    lba: sfo_lba,
                    len: self.param_sfo.len() as u32,
                    is_dir: false,
                },
                DirEntry {
                    name: "ICON0.PNG;1",
                    lba: icon_lba,
                    len: self.icon0.len() as u32,
                    is_dir: false,
                },
                DirEntry {
                    name: "SYSDIR",
                    lba: SYSDIR_LBA,
                    len: SECTOR_LEN as u32,
                    is_dir: true,
                },
            ],
        );
        write_directory(
            &mut image,
            SYSDIR_LBA,
            PSP_GAME_LBA,
            &[DirEntry {
                name: "EBOOT.BIN;1",
                lba: eboot_lba,
                len: self.eboot.len() as u32,
                is_dir: false,
            }],
        );

        // --- File payloads -------------------------------------------------
        write_at(&mut image, sfo_lba, &self.param_sfo);
        write_at(&mut image, icon_lba, &self.icon0);
        write_at(&mut image, eboot_lba, &self.eboot);

        image
    }
}

fn write_directory(image: &mut [u8], lba: u32, parent_lba: u32, children: &[DirEntry]) {
    let mut records = Vec::new();
    // Every directory opens with "." and ".." records.
    records.extend_from_slice(&dir_record(&DirEntry {
        name: "\0",
        lba,
        len: SECTOR_LEN as u32,
        is_dir: true,
    }));
    records.extend_from_slice(&dir_record(&DirEntry {
        name: "\u{1}",
        lba: parent_lba,
        len: SECTOR_LEN as u32,
        is_dir: true,
    }));
    for child in children {
        records.extend_from_slice(&dir_record(child));
    }
    assert!(
        records.len() <= SECTOR_LEN,
        "test fixture directory spans more than one sector"
    );
    image[lba as usize * SECTOR_LEN..][..records.len()].copy_from_slice(&records);
}

fn dir_record(entry: &DirEntry) -> Vec<u8> {
    let id: Vec<u8> = if entry.name == "\0" {
        vec![0]
    } else if entry.name == "\u{1}" {
        vec![1]
    } else {
        entry.name.as_bytes().to_vec()
    };

    let mut out = Vec::new();
    let record_len = 33 + id.len() + usize::from(!(33 + id.len()).is_multiple_of(2));
    out.push(record_len as u8);
    out.push(0); // extended attribute length
    let mut extent = [0u8; 8];
    write_both_endian_u32(&mut extent, entry.lba);
    out.extend_from_slice(&extent);
    let mut len = [0u8; 8];
    write_both_endian_u32(&mut len, entry.len);
    out.extend_from_slice(&len);
    out.extend_from_slice(&[125, 1, 1, 0, 0, 0, 0]); // 2025-01-01 00:00 GMT
    out.push(if entry.is_dir { 0x02 } else { 0x00 });
    out.push(0); // file unit size
    out.push(0); // interleave gap
    let mut seq = [0u8; 4];
    write_both_endian_u16(&mut seq, 1);
    out.extend_from_slice(&seq);
    out.push(id.len() as u8);
    out.extend_from_slice(&id);
    if out.len() % 2 != 0 {
        out.push(0);
    }
    debug_assert_eq!(out.len(), record_len);
    out
}

fn write_at(image: &mut [u8], lba: u32, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    image[lba as usize * SECTOR_LEN..][..bytes.len()].copy_from_slice(bytes);
}

fn sectors_for(len: usize) -> u32 {
    (len.div_ceil(SECTOR_LEN).max(1)) as u32
}

fn pad_string(field: &mut [u8], value: &str) {
    field.fill(b' ');
    let bytes = value.as_bytes();
    let n = bytes.len().min(field.len());
    field[..n].copy_from_slice(&bytes[..n]);
}

fn write_both_endian_u32(field: &mut [u8], value: u32) {
    field[0..4].copy_from_slice(&value.to_le_bytes());
    field[4..8].copy_from_slice(&value.to_be_bytes());
}

fn write_both_endian_u16(field: &mut [u8], value: u16) {
    field[0..2].copy_from_slice(&value.to_le_bytes());
    field[2..4].copy_from_slice(&value.to_be_bytes());
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

/// Wraps an ISO image in the CISO container, compressing nothing: every block
/// is stored with the "uncompressed" bit set. That exercises the index maths
/// and block plumbing without depending on a deflate implementation.
pub fn cso_store_only(iso: &[u8], block_size: u32) -> Vec<u8> {
    let block_count = iso.len().div_ceil(block_size as usize);
    let header_len = 0x18;
    let index_len = (block_count + 1) * 4;
    let data_start = header_len + index_len;

    let mut out = vec![0u8; data_start];
    out[0..4].copy_from_slice(b"CISO");
    out[4..8].copy_from_slice(&(header_len as u32).to_le_bytes());
    out[8..16].copy_from_slice(&(iso.len() as u64).to_le_bytes());
    out[16..20].copy_from_slice(&block_size.to_le_bytes());
    out[20] = 1; // version
    out[21] = 0; // index alignment shift

    for i in 0..block_count {
        let offset = out.len() as u32;
        // Top bit set marks the block as stored rather than deflated.
        let entry = offset | 0x8000_0000;
        out[header_len + i * 4..][..4].copy_from_slice(&entry.to_le_bytes());
        let start = i * block_size as usize;
        let end = ((i + 1) * block_size as usize).min(iso.len());
        out.extend_from_slice(&iso[start..end]);
    }
    // The trailing index entry marks where the last block ends.
    let end_entry = out.len() as u32;
    out[header_len + block_count * 4..][..4].copy_from_slice(&end_entry.to_le_bytes());
    out
}
