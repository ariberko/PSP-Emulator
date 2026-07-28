//! Reads PSP game metadata straight out of disc images and packages.
//!
//! The XMB game list needs each title's real name, disc ID and 144×80 icon —
//! the same data a real PSP reads off a UMD. That means parsing the formats
//! themselves, since no index file exists to consult:
//!
//! | Format | Path |
//! | ------ | ---- |
//! | `.iso` | ISO9660 → `PSP_GAME/PARAM.SFO`, `PSP_GAME/ICON0.PNG` |
//! | `.cso` | CISO block decompression → the above |
//! | `.pbp` | PBP section table → `PARAM.SFO`, `ICON0.PNG`, `PIC1.PNG` |
//! | `.elf` | no metadata; the file name is the title |
//!
//! Nothing here emulates anything: it is the metadata layer the shell renders,
//! while PPSSPP does the actual emulation.
//!
//! ```no_run
//! use psp_metadata::scan_library;
//!
//! let scan = scan_library(&["/home/me/ROMs".into()]);
//! for game in &scan.games {
//!     println!("{} [{}] {}", game.title, game.format.label(), game.path.display());
//! }
//! ```

pub mod cso;
pub mod game;
pub mod iso;
pub mod pbp;
pub mod scan;
pub mod sfo;

#[cfg(any(test, feature = "testkit"))]
pub mod testkit;

pub use cso::CsoReader;
pub use game::{Game, GameFormat};
pub use iso::{FileSource, IsoReader, ReadAt};
pub use pbp::{Pbp, PbpSection};
pub use scan::{scan_library, scan_library_with_depth, LibraryScan, ScanProblem};
pub use sfo::{Sfo, SfoValue};
