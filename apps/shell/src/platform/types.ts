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
