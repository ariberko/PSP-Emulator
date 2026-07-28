/**
 * Stand-in host for the browser build.
 *
 * The web demo on Base44 has no filesystem and no emulator to launch, but the
 * XMB should still be explorable — a hackathon judge clicking the download page
 * shouldn't land on an empty shell. So this serves a small synthetic library with
 * generated cover art and reports honestly that launching is unavailable.
 *
 * Titles here are deliberately generic placeholders rather than real games: this
 * is demo furniture, not a claim to ship anyone's content.
 */

import type {
  EmulatorStatus,
  Game,
  HostBridge,
  LibraryScan,
  PadProfile,
  Settings,
} from './types';

/**
 * Generates 144×80 cover art as an inline SVG data URL — the same dimensions as
 * a real ICON0, so the layout is exercised at true proportions.
 */
function coverArt(title: string, from: string, to: string): string {
  const initial = title.trim().charAt(0).toUpperCase() || '?';
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="144" height="80" viewBox="0 0 144 80">
    <defs>
      <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0%" stop-color="${from}"/>
        <stop offset="100%" stop-color="${to}"/>
      </linearGradient>
    </defs>
    <rect width="144" height="80" fill="url(#g)"/>
    <circle cx="116" cy="18" r="26" fill="rgba(255,255,255,0.12)"/>
    <text x="14" y="58" font-family="Helvetica, Arial, sans-serif" font-size="46"
          font-weight="700" fill="rgba(255,255,255,0.92)">${initial}</text>
  </svg>`;
  // encodeURIComponent keeps the '#' in colours from truncating the URL.
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/** 480×272 backdrop art, matching a real PIC1's dimensions. */
function backdropArt(from: string, to: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="480" height="272" viewBox="0 0 480 272">
    <defs>
      <linearGradient id="b" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0%" stop-color="${from}"/>
        <stop offset="100%" stop-color="${to}"/>
      </linearGradient>
    </defs>
    <rect width="480" height="272" fill="url(#b)"/>
    <circle cx="380" cy="60" r="120" fill="rgba(255,255,255,0.08)"/>
    <circle cx="90" cy="240" r="150" fill="rgba(0,0,0,0.12)"/>
  </svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

interface DemoSpec {
  title: string;
  discId: string;
  format: Game['format'];
  size: number;
  from: string;
  to: string;
}

const DEMO_SPECS: DemoSpec[] = [
  { title: 'Aurora Drift', discId: 'DEMO00001', format: 'iso', size: 1_395_864_371, from: '#3b6ea5', to: '#12304a' },
  { title: 'Blade Cadence', discId: 'DEMO00002', format: 'cso', size: 742_391_808, from: '#a33b52', to: '#3d1220' },
  { title: 'Cosmic Rally', discId: 'DEMO00003', format: 'iso', size: 1_073_741_824, from: '#c98a2e', to: '#4a2c08' },
  { title: 'Deep Field', discId: 'DEMO00004', format: 'cso', size: 512_000_000, from: '#2e8b7a', to: '#0b2e2a' },
  { title: 'Echo Runner', discId: 'DEMO00005', format: 'pbp', size: 68_157_440, from: '#6a4fa3', to: '#241a3d' },
  { title: 'Homebrew Sampler', discId: '', format: 'elf', size: 2_097_152, from: '#4a4a52', to: '#1c1c22' },
];

function demoGames(): Game[] {
  return DEMO_SPECS.map((spec) => ({
    id: spec.discId || spec.title,
    title: spec.title,
    path: `/demo/${spec.title.toLowerCase().replace(/\s+/g, '-')}.${spec.format}`,
    format: spec.format,
    size_bytes: spec.size,
    disc_id: spec.discId || null,
    disc_version: spec.discId ? '1.00' : null,
    category: spec.format === 'elf' ? null : 'UG',
    system_version: spec.discId ? '6.60' : null,
    parental_level: null,
    icon: coverArt(spec.title, spec.from, spec.to),
    background: backdropArt(spec.from, spec.to),
  }));
}

export function mockBridge(): HostBridge {
  let settings: Settings = {
    rom_paths: ['(demo library)'],
    ppsspp_path: null,
    fullscreen: true,
    sound_enabled: true,
    theme_override: null,
  };

  return {
    kind: 'browser',

    async scanLibrary(): Promise<LibraryScan> {
      // A short delay so the loading path is exercised rather than skipped.
      await delay(220);
      return { games: demoGames(), problems: [], missing_roots: [] };
    },

    async launchGame(game: Game): Promise<void> {
      // Refusing loudly is the honest behaviour: there is no emulator here.
      throw new Error(
        `${game.title} can't be launched in the browser demo — download the desktop app to play.`,
      );
    },

    async getSettings(): Promise<Settings> {
      return settings;
    },

    async saveSettings(patch: Partial<Settings>): Promise<Settings> {
      settings = { ...settings, ...patch };
      return settings;
    },

    async emulatorStatus(): Promise<EmulatorStatus> {
      return { found: false, path: null, version: null, source: null };
    },

    async addRomFolder(): Promise<Settings | null> {
      return null;
    },

    async hostVersion(): Promise<string> {
      return 'web demo';
    },

    async padProfile(): Promise<PadProfile> {
      // No filesystem in the browser, so there is no controls.ini to read. The
      // shell's built-in mapping covers this case on its own.
      return { source: null, buttons: {} };
    },
  };
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
