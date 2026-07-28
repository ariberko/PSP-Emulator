/**
 * Picks the host implementation at runtime.
 *
 * On the desktop the shell talks to Rust over Tauri's IPC. The same bundle is
 * also deployed to Base44 as the web demo, where there is no host at all — so a
 * mock stands in and the UI code never has to ask which one it is running under.
 */

import { convertFileSrc } from '@tauri-apps/api/core';

import { mockBridge } from './mock';
import type { HostBridge } from './types';

/** Tauri v2 exposes its IPC internals on the window before any user code runs. */
interface TauriWindow {
  __TAURI_INTERNALS__?: {
    invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  };
}

export function isTauri(): boolean {
  return Boolean((window as unknown as TauriWindow).__TAURI_INTERNALS__);
}

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const internals = (window as unknown as TauriWindow).__TAURI_INTERNALS__;
  if (!internals) {
    return Promise.reject(new Error('Tauri IPC is unavailable'));
  }
  return internals.invoke<T>(cmd, args);
}

/**
 * Bridge backed by the Rust commands in `apps/desktop/src-tauri`.
 *
 * Command names are the Rust function names; keep the two in step.
 */
const tauriBridge: HostBridge = {
  kind: 'tauri',
  scanLibrary: () => invoke('scan_library'),
  // Rust receives the path rather than the whole game: the host re-reads what it
  // needs, and passing an icon's base64 back over IPC would be wasteful.
  launchGame: (game) => invoke('launch_game', { path: game.path }),
  getSettings: () => invoke('get_settings'),
  saveSettings: (settings) => invoke('save_settings', { patch: settings }),
  emulatorStatus: () => invoke('emulator_status'),
  addRomFolder: () => invoke('add_rom_folder'),
  hostVersion: () => invoke('host_version'),
  padProfile: () => invoke('pad_profile'),
  scanMedia: () => invoke('scan_media'),
  addMediaFolder: () => invoke('add_media_folder'),
  // The Rust side widens the asset-protocol scope to the configured folders, so
  // this URL streams the file straight into an <img>, <audio> or <video>.
  mediaUrl: (item) => convertFileSrc(item.path),
  saveStates: () => invoke('save_states'),
};

export function createBridge(): HostBridge {
  return isTauri() ? tauriBridge : mockBridge();
}
