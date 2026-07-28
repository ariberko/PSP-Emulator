/**
 * Shapes crossing the boundary between the Rust host and the shell.
 *
 * These mirror the `serde` output of `psp-metadata` and the desktop crate's
 * command results. Keeping them in one file makes the contract easy to check
 * against the Rust side when either moves.
 */

export type GameFormat = 'iso' | 'cso' | 'pbp' | 'elf';

/** Mirrors `psp_metadata::Game`. */
export interface Game {
  id: string;
  title: string;
  path: string;
  format: GameFormat;
  size_bytes: number;
  disc_id: string | null;
  disc_version: string | null;
  category: string | null;
  system_version: string | null;
  parental_level: number | null;
  /** `data:` URL for ICON0, already base64-encoded by the host. */
  icon: string | null;
  /** `data:` URL for PIC1. */
  background: string | null;
}

/** Mirrors `psp_metadata::ScanProblem`. */
export interface ScanProblem {
  path: string;
  reason: string;
}

/** Mirrors `psp_metadata::LibraryScan`. */
export interface LibraryScan {
  games: Game[];
  problems: ScanProblem[];
  missing_roots: string[];
}

export interface Settings {
  /** Folders scanned for games. */
  rom_paths: string[];
  /** Folders scanned for photos, music and video. */
  media_paths: string[];
  /** Explicit PPSSPP binary path; null means "search the usual places". */
  ppsspp_path: string | null;
  /** Launch PPSSPP already in fullscreen. */
  fullscreen: boolean;
  sound_enabled: boolean;
  /** Overrides the month-derived theme when set. */
  theme_override: string | null;
}

/** Result of resolving the emulator binary, so the UI can report it. */
export interface EmulatorStatus {
  found: boolean;
  path: string | null;
  version: string | null;
  /** Where it was found: an explicit setting, PATH, or a known install location. */
  source: string | null;
}

/** Mirrors `psp_host::SaveState`. */
export interface SaveState {
  disc_id: string;
  disc_version: string | null;
  slot: number;
  path: string;
  screenshot: string | null;
  size_bytes: number;
  checksum: string;
  modified_ms: number | null;
}

/** Mirrors `psp_host::SaveStateScan`. */
export interface SaveStateScan {
  states: SaveState[];
  source: string | null;
}

export type MediaKind = 'photo' | 'music' | 'video';

/** Mirrors `psp_metadata::MediaItem`. */
export interface MediaItem {
  id: string;
  title: string;
  path: string;
  kind: MediaKind;
  size_bytes: number;
  extension: string;
}

/** Mirrors `psp_metadata::MediaScan`. */
export interface MediaScan {
  photos: MediaItem[];
  music: MediaItem[];
  videos: MediaItem[];
  missing_roots: string[];
}

/**
 * Controller mapping imported from PPSSPP's own `controls.ini`.
 *
 * Mirrors `psp_host::PadProfile`. Applied on top of the shell's built-in
 * mapping rather than replacing it — see `pad.ts`.
 */
export interface PadProfile {
  /** Path the mapping was read from, for display. Null when none was found. */
  source: string | null;
  /** Action name (`"confirm"`, `"up"`, …) to gamepad button indices. */
  buttons: Record<string, number[]>;
}

/**
 * Everything the shell needs from its host.
 *
 * Implemented by the Tauri bridge on the desktop and by a mock in the browser
 * demo, so the same UI code runs in both without branching.
 */
export interface HostBridge {
  readonly kind: 'tauri' | 'browser';
  scanLibrary(): Promise<LibraryScan>;
  launchGame(game: Game): Promise<void>;
  getSettings(): Promise<Settings>;
  saveSettings(settings: Partial<Settings>): Promise<Settings>;
  emulatorStatus(): Promise<EmulatorStatus>;
  /** Opens a folder picker and adds the result to `rom_paths`. */
  addRomFolder(): Promise<Settings | null>;
  hostVersion(): Promise<string>;
  /** PPSSPP's own controller mapping, if it has one. */
  padProfile(): Promise<PadProfile>;
  /** Photos, music and video in the configured media folders. */
  scanMedia(): Promise<MediaScan>;
  /** Opens a folder picker and adds the result to `media_paths`. */
  addMediaFolder(): Promise<Settings | null>;
  /**
   * A URL the webview can load for a local file.
   *
   * On the desktop this goes through Tauri's asset protocol so large files stream
   * rather than crossing IPC; in the browser there is no local file to serve.
   */
  mediaUrl(item: MediaItem): string | null;
  /** PPSSPP's save states on this machine. */
  saveStates(): Promise<SaveStateScan>;
}

/** Human-readable file size, e.g. `1.4 GB`. */
export function formatSize(bytes: number): string {
  if (bytes <= 0) {
    return '—';
  }
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  // Sub-10 values keep a decimal so "1.4 GB" doesn't collapse to "1 GB".
  const digits = value < 10 && unit > 0 ? 1 : 0;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

/** The line shown under a selected game: format, size, and disc ID when known. */
export function gameSublabel(game: Game): string {
  const parts = [game.format.toUpperCase(), formatSize(game.size_bytes)];
  if (game.disc_id) {
    parts.push(game.disc_id);
  }
  return parts.join('  ·  ');
}
