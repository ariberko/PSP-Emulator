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

/** Mirrors `psp_host::BundledRom`: one game shipped inside the installer. */
export interface BundledRom {
  file_name: string;
  size_bytes: number;
}

/** Mirrors `psp_host::bundled_roms::InstallFailure`. */
export interface InstallFailure {
  file_name: string;
  reason: string;
}

/** Mirrors `psp_host::InstallReport`. */
export interface InstallReport {
  target: string;
  installed: string[];
  already_present: string[];
  failed: InstallFailure[];
  bytes_copied: number;
}

/** Mirrors the `InstallOutcome` the `install_bundled_roms` command returns. */
export interface InstallOutcome {
  settings: Settings;
  report: InstallReport;
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
  /**
   * Games shipped inside this build, if any.
   *
   * Empty is a normal answer, not a failure: the Settings item describes what it
   * would install and disables itself when there is nothing to install.
   */
  bundledRoms(): Promise<BundledRom[]>;
  /** Copies the bundled games somewhere writable and adds that folder to the library. */
  installBundledRoms(): Promise<InstallOutcome>;
}

/**
 * One line summarising what an install did, for the Settings item's sublabel.
 *
 * Every branch has to be distinguishable, because "nothing was copied" has three
 * very different causes: it all worked already, it all failed, or the build ships
 * no games. Collapsing them into one message is how a broken install reads as a
 * successful one.
 */
export function describeInstall(report: InstallReport): string {
  const copied = report.installed.length;
  const present = report.already_present.length;
  const failed = report.failed.length;

  if (failed > 0) {
    const first = report.failed[0];
    const rest = failed > 1 ? ` (and ${failed - 1} more)` : '';
    return `${first.file_name} failed: ${first.reason}${rest}`;
  }
  if (copied > 0) {
    const size = report.bytes_copied > 0 ? `, ${formatSize(report.bytes_copied)}` : '';
    return `Installed ${copied} ${copied === 1 ? 'game' : 'games'}${size}`;
  }
  if (present > 0) {
    return `Already installed — ${present} ${present === 1 ? 'game' : 'games'} in place`;
  }
  return 'This build ships no games';
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
