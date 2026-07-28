//! Reading PPSSPP's own controller mapping.
//!
//! PPSSPP already stores whatever pad layout the user configured, in
//! `PSP/SYSTEM/controls.ini` under its memory-stick root. If someone has bound
//! ✕ to an unusual button there, the XMB agreeing with the game is strictly
//! better than the two disagreeing — so this reads that file rather than asking
//! the user to configure a second time.
//!
//! The file looks like:
//!
//! ```ini
//! [ControlMapping]
//! Up = 1-19,10-19
//! Cross = 1-32,10-96
//! Circle = 1-33,10-97
//! ```
//!
//! Each binding is `deviceId-keyCode`, comma-separated for multiple bindings.
//! Key codes are Android `AKEYCODE_*` values, which is what PPSSPP normalises
//! every platform's input to.
//!
//! Two deliberate simplifications:
//!
//! - **Device IDs are not enumerated.** PPSSPP's numbering has shifted between
//!   versions, and the only question here is "is this a pad rather than the
//!   keyboard or mouse", so anything that is not keyboard or mouse counts as a
//!   pad. That cannot go stale.
//! - **Axis bindings are ignored.** PPSSPP encodes them well above the button
//!   range; the shell reads sticks directly from the Gamepad API, so an imported
//!   axis binding would add nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// PPSSPP device IDs that are not controllers.
const DEVICE_KEYBOARD: i32 = 1;
const DEVICE_MOUSE: i32 = 2;

/// Anything at or above this is an axis, not a button.
const AXIS_CODE_FLOOR: i32 = 4000;

/// Android keycode to Standard Gamepad button index.
///
/// This is the same correspondence the Gamepad API's standard mapping defines, so
/// a pad PPSSPP sees as `BUTTON_A` is the one a browser reports at index 0.
const KEYCODE_TO_BUTTON: &[(i32, usize)] = &[
    (96, 0),   // BUTTON_A — bottom face
    (97, 1),   // BUTTON_B — right face
    (99, 2),   // BUTTON_X — left face
    (100, 3),  // BUTTON_Y — top face
    (102, 4),  // BUTTON_L1
    (103, 5),  // BUTTON_R1
    (104, 6),  // BUTTON_L2
    (105, 7),  // BUTTON_R2
    (109, 8),  // BUTTON_SELECT
    (108, 9),  // BUTTON_START
    (106, 10), // BUTTON_THUMBL
    (107, 11), // BUTTON_THUMBR
    (19, 12),  // DPAD_UP
    (20, 13),  // DPAD_DOWN
    (21, 14),  // DPAD_LEFT
    (22, 15),  // DPAD_RIGHT
    (110, 16), // BUTTON_MODE — the guide/PS button
];

/// Which PSP buttons the XMB cares about, and the action each drives.
const ACTIONS: &[(&str, Action)] = &[
    ("Cross", Action::Confirm),
    ("Circle", Action::Back),
    ("Up", Action::Up),
    ("Down", Action::Down),
    ("Left", Action::Left),
    ("Right", Action::Right),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Action {
    Confirm,
    Back,
    Up,
    Down,
    Left,
    Right,
}

impl Action {
    fn key(&self) -> &'static str {
        match self {
            Action::Confirm => "confirm",
            Action::Back => "back",
            Action::Up => "up",
            Action::Down => "down",
            Action::Left => "left",
            Action::Right => "right",
        }
    }
}

/// Gamepad button indices imported from PPSSPP, keyed by XMB action.
///
/// The shell applies these *on top of* its own defaults rather than replacing
/// them: a config for a different pad, or a keycode this table does not know,
/// must never take away a button that already worked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PadProfile {
    /// Where the mapping was read from, for display.
    pub source: Option<PathBuf>,
    /// Action name (`"confirm"`, `"up"`, …) to gamepad button indices.
    pub buttons: BTreeMap<String, Vec<usize>>,
}

impl PadProfile {
    pub fn is_empty(&self) -> bool {
        self.buttons.is_empty()
    }

    /// Count of imported bindings, for the "N bindings imported" reading.
    pub fn binding_count(&self) -> usize {
        self.buttons.values().map(Vec::len).sum()
    }
}

/// Candidate locations for `controls.ini`.
///
/// PPSSPP's memory-stick root varies by platform and by whether the install is
/// portable, so every plausible root is tried and the first that exists wins.
pub fn candidate_paths() -> Vec<PathBuf> {
    let suffix = Path::new("PSP").join("SYSTEM").join("controls.ini");
    let mut roots: Vec<PathBuf> = Vec::new();

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        // Linux and macOS both use this by default.
        roots.push(home.join(".config").join("ppsspp"));
        roots.push(home.join(".ppsspp"));
        roots.push(
            home.join("Library")
                .join("Application Support")
                .join("PPSSPP"),
        );
        roots.push(home.join("Documents").join("PPSSPP"));
    }
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
        roots.push(config.join("ppsspp"));
    }
    if let Some(profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        roots.push(profile.join("Documents").join("PPSSPP"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
        roots.push(appdata.join("PPSSPP"));
    }

    roots.into_iter().map(|root| root.join(&suffix)).collect()
}

/// Finds PPSSPP's `controls.ini`, if it has one.
///
/// `emulator` is the resolved PPSSPP binary; a portable Windows install keeps its
/// memory stick in a `memstick/` folder beside the executable, which no
/// home-directory guess would find.
pub fn find_controls_ini(emulator: Option<&Path>) -> Option<PathBuf> {
    if let Some(binary) = emulator {
        if let Some(dir) = binary.parent() {
            let portable = dir
                .join("memstick")
                .join("PSP")
                .join("SYSTEM")
                .join("controls.ini");
            if portable.is_file() {
                return Some(portable);
            }
        }
    }

    candidate_paths().into_iter().find(|path| path.is_file())
}

/// Reads and translates PPSSPP's mapping.
///
/// Returns an empty profile rather than an error when there is nothing to read:
/// having no PPSSPP config is the normal case for a fresh install, and the
/// shell's own defaults already work.
pub fn load_pad_profile(emulator: Option<&Path>) -> PadProfile {
    let Some(path) = find_controls_ini(emulator) else {
        return PadProfile::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return PadProfile::default();
    };

    let mut profile = parse_controls_ini(&text);
    if !profile.is_empty() {
        profile.source = Some(path);
    }
    profile
}

/// Parses a `controls.ini`, keeping only pad bindings for the actions the XMB uses.
pub fn parse_controls_ini(text: &str) -> PadProfile {
    let mut buttons: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut in_mapping = false;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') {
            // Only [ControlMapping] holds bindings; other sections are settings.
            in_mapping = line.eq_ignore_ascii_case("[ControlMapping]");
            continue;
        }
        if !in_mapping {
            continue;
        }

        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();

        let Some(action) = ACTIONS
            .iter()
            .find(|(psp, _)| psp.eq_ignore_ascii_case(name))
            .map(|(_, action)| *action)
        else {
            continue;
        };

        for binding in value.split(',') {
            if let Some(index) = pad_button_index(binding.trim()) {
                let entry = buttons.entry(action.key().to_string()).or_default();
                // The same index can appear via several bindings; keep it once.
                if !entry.contains(&index) {
                    entry.push(index);
                }
            }
        }
    }

    for indices in buttons.values_mut() {
        indices.sort_unstable();
    }

    PadProfile {
        source: None,
        buttons,
    }
}

/// Translates one `deviceId-keyCode` binding into a gamepad button index.
///
/// `None` for keyboard and mouse bindings, axis codes, and keycodes outside the
/// standard-gamepad correspondence.
fn pad_button_index(binding: &str) -> Option<usize> {
    // Negative device ids are not something PPSSPP writes, so a single '-' splits
    // device from keycode cleanly.
    let (device, code) = binding.split_once('-')?;
    let device: i32 = device.trim().parse().ok()?;
    let code: i32 = code.trim().parse().ok()?;

    if device == DEVICE_KEYBOARD || device == DEVICE_MOUSE {
        return None;
    }
    if code >= AXIS_CODE_FLOOR {
        return None;
    }

    KEYCODE_TO_BUTTON
        .iter()
        .find(|(keycode, _)| *keycode == code)
        .map(|(_, index)| *index)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trimmed but realistically shaped controls.ini.
    const SAMPLE: &str = r#"
[ControlMapping]
Up = 1-19,10-19
Down = 1-20,10-20
Left = 1-21,10-21
Right = 1-22,10-22
Circle = 1-33,10-97
Cross = 1-32,10-96
Square = 1-31,10-99
Triangle = 1-30,10-100
Start = 1-62,10-108
Select = 1-61,10-109

[Graphics]
Fullscreen = True
"#;

    #[test]
    fn imports_pad_bindings_for_the_actions_the_xmb_uses() {
        let profile = parse_controls_ini(SAMPLE);
        assert_eq!(profile.buttons.get("confirm"), Some(&vec![0]));
        assert_eq!(profile.buttons.get("back"), Some(&vec![1]));
        assert_eq!(profile.buttons.get("up"), Some(&vec![12]));
        assert_eq!(profile.buttons.get("down"), Some(&vec![13]));
        assert_eq!(profile.buttons.get("left"), Some(&vec![14]));
        assert_eq!(profile.buttons.get("right"), Some(&vec![15]));
    }

    #[test]
    fn ignores_buttons_the_xmb_has_no_action_for() {
        // Square, Triangle, Start and Select are all present in the sample.
        let profile = parse_controls_ini(SAMPLE);
        assert_eq!(profile.buttons.len(), 6);
        assert!(!profile.buttons.contains_key("square"));
    }

    #[test]
    fn ignores_keyboard_bindings() {
        // Every action in the sample also has a 1-xx keyboard binding, and none of
        // those keycodes may be mistaken for a pad button.
        let profile = parse_controls_ini("[ControlMapping]\nCross = 1-32\n");
        assert!(profile.is_empty());
    }

    #[test]
    fn ignores_mouse_bindings() {
        assert!(parse_controls_ini("[ControlMapping]\nCross = 2-96\n").is_empty());
    }

    #[test]
    fn ignores_axis_bindings() {
        // The shell reads sticks straight from the Gamepad API, so an axis binding
        // would contribute nothing.
        assert!(parse_controls_ini("[ControlMapping]\nUp = 10-4003\n").is_empty());
    }

    #[test]
    fn ignores_bindings_outside_the_control_mapping_section() {
        let text = "[Graphics]\nCross = 10-96\n";
        assert!(parse_controls_ini(text).is_empty());
    }

    #[test]
    fn reads_a_remapped_cross() {
        // The whole point: someone who bound ✕ to the top face button should get
        // that button working in the XMB too.
        let profile = parse_controls_ini("[ControlMapping]\nCross = 10-100\n");
        assert_eq!(profile.buttons.get("confirm"), Some(&vec![3]));
    }

    #[test]
    fn keeps_multiple_pad_bindings_for_one_action() {
        let profile = parse_controls_ini("[ControlMapping]\nCross = 10-96,11-100\n");
        assert_eq!(profile.buttons.get("confirm"), Some(&vec![0, 3]));
    }

    #[test]
    fn deduplicates_the_same_button_bound_twice() {
        // Two pads bound to the same physical button yield one index, not two.
        let profile = parse_controls_ini("[ControlMapping]\nCross = 10-96,11-96\n");
        assert_eq!(profile.buttons.get("confirm"), Some(&vec![0]));
    }

    #[test]
    fn treats_any_non_keyboard_device_as_a_pad() {
        // PPSSPP's device numbering has moved between versions, so the parser must
        // not depend on a specific pad id.
        for device in [10, 11, 20, 21, 30] {
            let text = format!("[ControlMapping]\nCross = {device}-96\n");
            assert_eq!(
                parse_controls_ini(&text).buttons.get("confirm"),
                Some(&vec![0]),
                "device {device}"
            );
        }
    }

    #[test]
    fn tolerates_whitespace_comments_and_crlf() {
        let text = "; a comment\r\n[ControlMapping]\r\n  Cross  =  10-96  \r\n# another\r\n";
        assert_eq!(
            parse_controls_ini(text).buttons.get("confirm"),
            Some(&vec![0])
        );
    }

    #[test]
    fn section_and_key_names_are_case_insensitive() {
        let text = "[controlmapping]\ncross = 10-96\n";
        assert_eq!(
            parse_controls_ini(text).buttons.get("confirm"),
            Some(&vec![0])
        );
    }

    #[test]
    fn ignores_malformed_lines_without_losing_the_rest() {
        // A truncated or hand-edited file should still yield what it can.
        let text = "[ControlMapping]\nnonsense\nCross = \nCircle = 10-97\nUp = 10-notanumber\n";
        let profile = parse_controls_ini(text);
        assert_eq!(profile.buttons.get("back"), Some(&vec![1]));
        assert_eq!(profile.buttons.len(), 1);
    }

    #[test]
    fn an_unknown_keycode_is_skipped_rather_than_guessed() {
        assert!(parse_controls_ini("[ControlMapping]\nCross = 10-999\n").is_empty());
    }

    #[test]
    fn an_empty_file_yields_an_empty_profile() {
        let profile = parse_controls_ini("");
        assert!(profile.is_empty());
        assert_eq!(profile.binding_count(), 0);
        assert_eq!(profile.source, None);
    }

    #[test]
    fn counts_imported_bindings() {
        assert_eq!(parse_controls_ini(SAMPLE).binding_count(), 6);
    }

    #[test]
    fn loading_with_no_ppsspp_config_present_is_empty_not_an_error() {
        let profile = load_pad_profile(Some(Path::new("/nonexistent/PPSSPPSDL")));
        // The host may genuinely have PPSSPP installed, so only assert that this
        // does not fail; the parse paths above cover the content.
        let _ = profile.binding_count();
    }

    #[test]
    fn finds_a_portable_install_beside_the_binary() {
        let dir = tempfile::tempdir().unwrap();
        let system = dir.path().join("memstick").join("PSP").join("SYSTEM");
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(system.join("controls.ini"), SAMPLE).unwrap();

        let binary = dir.path().join("PPSSPPWindows64.exe");
        std::fs::write(&binary, b"fake").unwrap();

        let found = find_controls_ini(Some(&binary)).expect("portable config found");
        assert_eq!(found, system.join("controls.ini"));
    }

    #[test]
    fn records_where_the_mapping_came_from() {
        let dir = tempfile::tempdir().unwrap();
        let system = dir.path().join("memstick").join("PSP").join("SYSTEM");
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(system.join("controls.ini"), SAMPLE).unwrap();
        let binary = dir.path().join("PPSSPPSDL");
        std::fs::write(&binary, b"fake").unwrap();

        let profile = load_pad_profile(Some(&binary));
        assert_eq!(profile.source, Some(system.join("controls.ini")));
        assert_eq!(profile.buttons.get("confirm"), Some(&vec![0]));
    }

    #[test]
    fn candidate_paths_all_end_at_controls_ini() {
        for path in candidate_paths() {
            assert!(path.ends_with("PSP/SYSTEM/controls.ini"), "{path:?}");
        }
    }
}
