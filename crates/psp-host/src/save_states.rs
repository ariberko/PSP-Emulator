//! Finding PPSSPP's save states.
//!
//! PPSSPP writes save states to `PSP/PPSSPP_STATE/` under its memory-stick root,
//! naming each one `<DISC_ID>_<DISC_VERSION>_<slot>.ppst` with a matching `.jpg`
//! thumbnail beside it. Everything the `SaveState` entity needs — disc, slot,
//! size, digest — is therefore derivable locally, with no help from the server.
//!
//! This is the half of cloud sync that can be built and tested without a
//! reachable backend, and it is also the half where the bugs live: filename
//! parsing, pairing thumbnails, and deciding which states actually need
//! uploading. The transport is a thin layer on top.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::ppsspp_config;

/// Extension PPSSPP gives its save states.
const STATE_EXTENSION: &str = "ppst";
/// PPSSPP exposes five slots, numbered from zero.
const MAX_SLOT: u32 = 4;

/// One save state found on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaveState {
    /// Disc ID the state belongs to, e.g. `UCUS98618`.
    pub disc_id: String,
    /// Disc version as PPSSPP recorded it, e.g. `1.00`.
    pub disc_version: Option<String>,
    pub slot: u32,
    pub path: PathBuf,
    /// The `.jpg` PPSSPP writes alongside, when present.
    pub screenshot: Option<PathBuf>,
    pub size_bytes: u64,
    /// SHA-256 of the state, so an unchanged state need not be re-uploaded.
    pub checksum: String,
    /// File modification time as a Unix millisecond timestamp.
    pub modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SaveStateScan {
    pub states: Vec<SaveState>,
    /// Where the states were found, for display.
    pub source: Option<PathBuf>,
}

impl SaveStateScan {
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
}

/// Locates PPSSPP's save-state directory.
///
/// Derived from wherever `controls.ini` was found, since both live under the same
/// memory-stick root — which avoids repeating the platform guesswork.
pub fn find_state_dir(emulator: Option<&Path>) -> Option<PathBuf> {
    let controls = ppsspp_config::find_controls_ini(emulator)?;
    // .../PSP/SYSTEM/controls.ini -> .../PSP/PPSSPP_STATE
    let psp_root = controls.parent()?.parent()?;
    let states = psp_root.join("PPSSPP_STATE");
    states.is_dir().then_some(states)
}

/// Scans for save states.
///
/// Returns an empty scan when PPSSPP has none, which is the normal case before a
/// game has been played.
pub fn scan_save_states(emulator: Option<&Path>) -> SaveStateScan {
    let Some(dir) = find_state_dir(emulator) else {
        return SaveStateScan::default();
    };
    let mut scan = scan_dir(&dir);
    if !scan.is_empty() {
        scan.source = Some(dir);
    }
    scan
}

/// Scans a specific directory. Exposed so tests need no real PPSSPP install.
pub fn scan_dir(dir: &Path) -> SaveStateScan {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return SaveStateScan::default();
    };

    let mut states = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            != Some(STATE_EXTENSION.to_string())
        {
            continue;
        }

        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(parsed) = parse_state_name(name) else {
            // A file that does not follow PPSSPP's naming is not something this can
            // attribute to a game, so it is skipped rather than guessed at.
            continue;
        };

        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };

        // PPSSPP writes "<state>.jpg", i.e. the thumbnail keeps the .ppst stem.
        let screenshot = ["jpg", "jpeg", "png"]
            .iter()
            .map(|ext| path.with_extension(ext))
            .find(|candidate| candidate.is_file());

        states.push(SaveState {
            disc_id: parsed.disc_id,
            disc_version: parsed.disc_version,
            slot: parsed.slot,
            path: path.clone(),
            screenshot,
            size_bytes: metadata.len(),
            checksum: sha256_hex(&bytes),
            modified_ms: modified_ms(&metadata),
        });
    }

    // Grouped by game then slot, which is how the UI lists them.
    states.sort_by(|a, b| a.disc_id.cmp(&b.disc_id).then_with(|| a.slot.cmp(&b.slot)));

    SaveStateScan {
        states,
        source: None,
    }
}

struct ParsedName {
    disc_id: String,
    disc_version: Option<String>,
    slot: u32,
}

/// Parses `<DISC_ID>_<DISC_VERSION>_<slot>`.
///
/// Split from the right rather than the left: a disc ID is normally free of
/// underscores, but homebrew IDs are not always, and the slot and version are
/// always the last two fields.
fn parse_state_name(stem: &str) -> Option<ParsedName> {
    let (rest, slot) = stem.rsplit_once('_')?;
    let slot: u32 = slot.parse().ok()?;
    if slot > MAX_SLOT {
        return None;
    }

    // The version is the next field from the right, but only if it looks like one;
    // some builds omit it entirely.
    match rest.rsplit_once('_') {
        Some((disc_id, version)) if looks_like_version(version) && !disc_id.is_empty() => {
            Some(ParsedName {
                disc_id: disc_id.to_string(),
                disc_version: Some(version.to_string()),
                slot,
            })
        }
        _ => (!rest.is_empty()).then(|| ParsedName {
            disc_id: rest.to_string(),
            disc_version: None,
            slot,
        }),
    }
}

fn looks_like_version(value: &str) -> bool {
    !value.is_empty()
        && value.contains('.')
        && value.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn modified_ms(metadata: &std::fs::Metadata) -> Option<u64> {
    let modified = metadata.modified().ok()?;
    let since_epoch = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(since_epoch.as_millis() as u64)
}

/// States that differ from what the server already holds.
///
/// `remote` maps `"<disc_id>:<slot>"` to the checksum the server has. Comparing
/// digests rather than timestamps means a state restored from another machine, or
/// a clock that disagrees, does not cause a pointless re-upload.
pub fn states_needing_upload<'a>(
    states: &'a [SaveState],
    remote: &std::collections::BTreeMap<String, String>,
) -> Vec<&'a SaveState> {
    states
        .iter()
        .filter(|state| remote.get(&state.key()) != Some(&state.checksum))
        .collect()
}

impl SaveState {
    /// Key identifying the slot this state occupies, matching the server's row.
    pub fn key(&self) -> String {
        format!("{}:{}", self.disc_id, self.slot)
    }
}

/// SHA-256, as lower-case hex.
///
/// Implemented here rather than pulled in as a dependency: it is one well-specified
/// compression function, the only consumer is this module, and it keeps the
/// desktop app's dependency tree smaller. Verified against the standard vectors.
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Pad to a multiple of 64 bytes: a 0x80 byte, zeros, then the bit length.
    let mut message = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    let mut w = [0u32; 64];
    for chunk in message.chunks_exact(64) {
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes(word.try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ ((!v[4]) & v[6]);
            let temp1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let temp2 = s0.wrapping_add(maj);

            v[7] = v[6];
            v[6] = v[5];
            v[5] = v[4];
            v[4] = v[3].wrapping_add(temp1);
            v[3] = v[2];
            v[2] = v[1];
            v[1] = v[0];
            v[0] = temp1.wrapping_add(temp2);
        }

        for (slot, value) in h.iter_mut().zip(v) {
            *slot = slot.wrapping_add(value);
        }
    }

    h.iter().map(|word| format!("{word:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn sha256_matches_the_standard_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha256_handles_a_message_spanning_multiple_blocks() {
        // 1000 'a's, which crosses several 64-byte blocks and a padding boundary.
        let data = vec![b'a'; 1000];
        assert_eq!(
            sha256_hex(&data),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }

    #[test]
    fn sha256_pads_a_55_and_56_byte_message_correctly() {
        // 56 bytes is the boundary where padding needs an extra block.
        assert_eq!(
            sha256_hex(&[b'a'; 55]),
            "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318"
        );
        assert_eq!(
            sha256_hex(&[b'a'; 56]),
            "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"
        );
    }

    #[test]
    fn parses_ppsspps_state_naming() {
        let parsed = parse_state_name("ULUS10041_1.00_0").unwrap();
        assert_eq!(parsed.disc_id, "ULUS10041");
        assert_eq!(parsed.disc_version.as_deref(), Some("1.00"));
        assert_eq!(parsed.slot, 0);
    }

    #[test]
    fn parses_a_name_with_no_version() {
        let parsed = parse_state_name("HOMEBREW_2").unwrap();
        assert_eq!(parsed.disc_id, "HOMEBREW");
        assert_eq!(parsed.disc_version, None);
        assert_eq!(parsed.slot, 2);
    }

    #[test]
    fn keeps_underscores_inside_a_disc_id() {
        // Splitting from the left would truncate an id like this.
        let parsed = parse_state_name("MY_HOMEBREW_APP_1.00_3").unwrap();
        assert_eq!(parsed.disc_id, "MY_HOMEBREW_APP");
        assert_eq!(parsed.slot, 3);
    }

    #[test]
    fn rejects_a_slot_out_of_range() {
        // PPSSPP has five slots; anything else is not a state this understands.
        assert!(parse_state_name("ULUS10041_1.00_9").is_none());
    }

    #[test]
    fn rejects_a_name_with_no_slot() {
        assert!(parse_state_name("ULUS10041").is_none());
        assert!(parse_state_name("ULUS10041_1.00_x").is_none());
    }

    #[test]
    fn does_not_treat_a_non_numeric_field_as_a_version() {
        // "beta" is part of the id, not a version, so it must stay in the id.
        let parsed = parse_state_name("GAME_beta_1").unwrap();
        assert_eq!(parsed.disc_id, "GAME_beta");
        assert_eq!(parsed.disc_version, None);
    }

    #[test]
    fn scans_a_directory_of_states() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ULUS10041_1.00_0.ppst"), b"state zero").unwrap();
        std::fs::write(dir.path().join("ULUS10041_1.00_1.ppst"), b"state one").unwrap();
        // Not a state, and must be ignored.
        std::fs::write(dir.path().join("readme.txt"), b"x").unwrap();

        let scan = scan_dir(dir.path());
        assert_eq!(scan.states.len(), 2);
        assert_eq!(scan.states[0].slot, 0);
        assert_eq!(scan.states[1].slot, 1);
        assert_eq!(scan.states[0].disc_id, "ULUS10041");
        assert_eq!(scan.states[0].size_bytes, 10);
        assert_eq!(scan.states[0].checksum, sha256_hex(b"state zero"));
    }

    #[test]
    fn pairs_a_state_with_its_screenshot() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ULUS10041_1.00_0.ppst"), b"state").unwrap();
        std::fs::write(dir.path().join("ULUS10041_1.00_0.jpg"), b"thumb").unwrap();

        let scan = scan_dir(dir.path());
        assert_eq!(
            scan.states[0].screenshot,
            Some(dir.path().join("ULUS10041_1.00_0.jpg"))
        );
    }

    #[test]
    fn a_state_with_no_screenshot_is_still_found() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ULUS10041_1.00_0.ppst"), b"state").unwrap();
        assert_eq!(scan_dir(dir.path()).states[0].screenshot, None);
    }

    #[test]
    fn skips_files_that_do_not_follow_the_naming() {
        // Attributing these to a game would mean guessing, so they are skipped.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mystery.ppst"), b"x").unwrap();
        assert!(scan_dir(dir.path()).is_empty());
    }

    #[test]
    fn sorts_by_game_then_slot() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "ZZZZ00001_1.00_1.ppst",
            "AAAA00001_1.00_2.ppst",
            "AAAA00001_1.00_0.ppst",
        ] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        let keys: Vec<_> = scan_dir(dir.path())
            .states
            .iter()
            .map(SaveState::key)
            .collect();
        assert_eq!(keys, vec!["AAAA00001:0", "AAAA00001:2", "ZZZZ00001:1"]);
    }

    #[test]
    fn an_extension_in_upper_case_is_still_a_state() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ULUS10041_1.00_0.PPST"), b"x").unwrap();
        assert_eq!(scan_dir(dir.path()).states.len(), 1);
    }

    #[test]
    fn a_missing_directory_is_empty_not_an_error() {
        assert!(scan_dir(Path::new("/nonexistent/PPSSPP_STATE")).is_empty());
    }

    #[test]
    fn only_changed_states_need_uploading() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("AAAA00001_1.00_0.ppst"), b"unchanged").unwrap();
        std::fs::write(dir.path().join("AAAA00001_1.00_1.ppst"), b"changed").unwrap();
        let scan = scan_dir(dir.path());

        let mut remote = BTreeMap::new();
        // Slot 0 already matches; slot 1 holds something else.
        remote.insert("AAAA00001:0".to_string(), sha256_hex(b"unchanged"));
        remote.insert("AAAA00001:1".to_string(), sha256_hex(b"something older"));

        let pending = states_needing_upload(&scan.states, &remote);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].slot, 1);
    }

    #[test]
    fn everything_needs_uploading_when_the_server_has_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("AAAA00001_1.00_0.ppst"), b"x").unwrap();
        let scan = scan_dir(dir.path());
        assert_eq!(
            states_needing_upload(&scan.states, &BTreeMap::new()).len(),
            1
        );
    }

    #[test]
    fn finds_the_state_dir_beside_a_portable_install() {
        let dir = tempfile::tempdir().unwrap();
        let psp = dir.path().join("memstick").join("PSP");
        std::fs::create_dir_all(psp.join("SYSTEM")).unwrap();
        std::fs::write(
            psp.join("SYSTEM").join("controls.ini"),
            b"[ControlMapping]\n",
        )
        .unwrap();
        std::fs::create_dir_all(psp.join("PPSSPP_STATE")).unwrap();

        let binary = dir.path().join("PPSSPPSDL");
        std::fs::write(&binary, b"fake").unwrap();

        assert_eq!(
            find_state_dir(Some(&binary)),
            Some(psp.join("PPSSPP_STATE"))
        );
    }
}
