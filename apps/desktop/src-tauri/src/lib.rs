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
use psp_metadata::LibraryScan;
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
    let Some(folder) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    // A picked path may be a URI on some platforms; only a real filesystem path
    // is useful for scanning.
    let Ok(path) = folder.into_path() else {
        return Err("That folder can't be read from disk".to_string());
    };

    let mut settings = state.store.load();
    if !settings.add_rom_path(path) {
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
            emulator_status,
            launch_game,
            add_rom_folder,
            host_version,
            pad_profile
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the PSP-Emulator shell");
}
