// Suppresses the console window that Windows would otherwise open alongside the
// GUI in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    psp_emulator_desktop::run();
}
