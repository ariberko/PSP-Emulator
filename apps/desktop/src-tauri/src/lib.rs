//! Tauri command surface.
//!
//! Intentionally thin: every command translates IPC arguments into a `psp_host`
//! call and maps the result into something the webview can consume. All the real
//! behaviour lives in `psp-host`, which builds and tests without system webview
//! libraries — so keeping this layer free of logic keeps that logic testable.
//!
//! Command names here must match the strings in `apps/shell/src/platform/bridge.ts`.

use std::path::PathBuf;

use psp_host::{emulator, settings::SettingsPatch, Settings, Store};
use psp_metadata::{LibraryScan, MediaScan};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

/// Shared app state: where settings live.
struct AppState {
    store: Store,
}

/// Errors are returned as plain strings — the shell only ever displays them.
type CommandResult<T> = Result<T, String>;

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.store.load()
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, patch: SettingsPatch) -> CommandResult<Settings> {
    state.store.update(patch).map_err(|e| e.to_string())
}

#[tauri::command]
fn scan_library(state: State<'_, AppState>) -> LibraryScan {
    psp_host::scan(&state.store.load())
}

/// Photos, music and video in the configured media folders.
///
/// Also widens the asset-protocol scope to those folders, so the webview can load
/// the files directly with `<img>`, `<audio>` and `<video>`. Streaming through the
/// protocol rather than base64 over IPC is what makes a 2 GB video playable at
/// all, and scoping to exactly the configured roots keeps the rest of the disk
/// unreachable from the page.
#[tauri::command]
fn scan_media(app: tauri::AppHandle, state: State<'_, AppState>) -> MediaScan {
    let settings = state.store.load();
    let scope = app.asset_protocol_scope();
    for root in &settings.media_paths {
        // Errors here mean a folder vanished between the pick and the scan; the
        // scan itself reports it as a missing root.
        let _ = scope.allow_directory(root, true);
    }
    psp_host::scan_media(&settings)
}

#[tauri::command]
fn emulator_status(state: State<'_, AppState>) -> emulator::EmulatorStatus {
    let settings = state.store.load();
    emulator::resolve(settings.ppsspp_path.as_deref())
}

#[tauri::command]
fn launch_game(state: State<'_, AppState>, path: PathBuf) -> CommandResult<()> {
    let settings = state.store.load();
    emulator::launch(settings.ppsspp_path.as_deref(), &path, settings.fullscreen)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Opens a folder picker and appends the choice to the ROM paths.
///
/// Returns `None` when the user cancels, which the shell reports rather than
/// treating as an error.
#[tauri::command]
async fn add_rom_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Option<Settings>> {
    pick_folder_into(app, state, FolderKind::Rom).await
}

/// Opens a folder picker and appends the choice to the media paths.
#[tauri::command]
async fn add_media_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Option<Settings>> {
    pick_folder_into(app, state, FolderKind::Media).await
}

enum FolderKind {
    Rom,
    Media,
}

/// Shared picker flow for both folder kinds.
async fn pick_folder_into(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    kind: FolderKind,
) -> CommandResult<Option<Settings>> {
    let Some(folder) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    // A picked path may be a URI on some platforms; only a real filesystem path
    // is useful for scanning.
    let Ok(path) = folder.into_path() else {
        return Err("That folder can't be read from disk".to_string());
    };

    let mut settings = state.store.load();
    let added = match kind {
        FolderKind::Rom => settings.add_rom_path(path.clone()),
        FolderKind::Media => {
            // Widen the asset scope immediately so media in a freshly picked
            // folder is loadable without waiting for the next scan.
            let _ = app.asset_protocol_scope().allow_directory(&path, true);
            settings.add_media_path(path)
        }
    };

    if !added {
        // Already configured — report success with settings unchanged rather than
        // making the UI show an error for a harmless repeat.
        return Ok(Some(settings));
    }
    state.store.save(&settings).map_err(|e| e.to_string())?;
    Ok(Some(settings))
}

#[tauri::command]
fn host_version() -> String {
    psp_host::host_version()
}

/// Where the games that ship with the app live inside the installation.
///
/// A resource path is the only thing here that needs Tauri, which is why the
/// installer itself takes both directories as parameters.
///
/// In a release bundle this is the only location. During `tauri dev` there is no
/// bundle, so fall back to the folder in the checkout — otherwise the feature
/// would be untestable without building an installer.
fn bundled_rom_dir(app: &tauri::AppHandle) -> PathBuf {
    if let Ok(path) = app
        .path()
        .resolve(BUNDLED_ROMS, tauri::path::BaseDirectory::Resource)
    {
        if path.is_dir() {
            return path;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(BUNDLED_ROMS)
}

/// Folder name used in both the bundle and the repository, so the dev fallback
/// above and `tauri.conf.json`'s `resources` entry cannot drift apart.
const BUNDLED_ROMS: &str = "demo-roms";

/// Where installed games are copied to: writable, and survives an app update.
fn games_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("Games"))
        .map_err(|e| format!("Could not find a writable data folder: {e}"))
}

/// The games this build ships with, so the UI can offer them by name and size —
/// or say plainly that there are none rather than showing a button that does
/// nothing.
#[tauri::command]
fn bundled_roms(app: tauri::AppHandle) -> Vec<psp_host::BundledRom> {
    psp_host::bundled_roms::list(&bundled_rom_dir(&app))
}

/// Copies the bundled games somewhere writable and adds that folder to the library.
#[derive(serde::Serialize)]
struct InstallOutcome {
    settings: Settings,
    report: psp_host::InstallReport,
}

#[tauri::command]
fn install_bundled_roms(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<InstallOutcome> {
    let target = games_dir(&app)?;
    let (settings, report) =
        psp_host::install_bundled_roms(&state.store, &bundled_rom_dir(&app), &target)
            .map_err(|e| format!("Could not install the bundled games: {e}"))?;
    Ok(InstallOutcome { settings, report })
}

/// PPSSPP's save states, the local half of cloud sync.
#[tauri::command]
fn save_states(state: State<'_, AppState>) -> psp_host::SaveStateScan {
    psp_host::save_states(&state.store.load())
}

/// The controller mapping PPSSPP already has, so the XMB can agree with the game.
#[tauri::command]
fn pad_profile(state: State<'_, AppState>) -> psp_host::PadProfile {
    psp_host::pad_profile(&state.store.load())
}

/// Builds and runs the desktop app.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Settings live in the OS config directory, which only Tauri can
            // resolve portably; psp-host takes the directory as a parameter so it
            // stays testable.
            let config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&config_dir)?;
            app.manage(AppState {
                store: Store::new(&config_dir),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            scan_library,
            scan_media,
            emulator_status,
            launch_game,
            add_rom_folder,
            add_media_folder,
            host_version,
            pad_profile,
            save_states,
            bundled_roms,
            install_bundled_roms
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the PSP-Emulator shell");
}
