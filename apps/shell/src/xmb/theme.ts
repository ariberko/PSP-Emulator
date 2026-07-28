/**
 * The XMB's month-coloured backgrounds.
 *
 * A real PSP recolours its wallpaper according to the system clock's month, so
 * the console looks different in April than in October. Reproducing that is one
 * of the details that makes the shell read as a PSP instead of a dark-mode menu.
 *
 * These are hand-matched approximations of the stock XMB palette, not extracted
 * Sony assets — close in hue and mood rather than pixel-exact.
 */

export interface XmbTheme {
  name: string;
  /** Vertical background gradient, top to bottom. */
  backdrop: [string, string, string];
  /** Wave ribbon colour. */
  wave: string;
  /** Glow behind the selected icon. */
  glow: string;
}

const THEMES: XmbTheme[] = [
  // January — pale winter blue
  {
    name: 'January',
    backdrop: ['#7fa9c8', '#3f6d95', '#123047'],
    wave: '#cfe6f5',
    glow: '#bfe0f7',
  },
  // February — pink
  {
    name: 'February',
    backdrop: ['#c98bb0', '#8e4a76', '#3d1730'],
    wave: '#f4cfe4',
    glow: '#f7c2df',
  },
  // March — fresh yellow-green
  {
    name: 'March',
    backdrop: ['#a8c07a', '#6b8b3f', '#243515'],
    wave: '#e2f0c2',
    glow: '#d8f0a8',
  },
  // April — cherry blossom
  {
    name: 'April',
    backdrop: ['#e2a8bd', '#a86680', '#43202f'],
    wave: '#ffdfe9',
    glow: '#ffd0e2',
  },
  // May — green
  {
    name: 'May',
    backdrop: ['#7cba86', '#3f8250', '#12301c'],
    wave: '#cdf0d5',
    glow: '#b8f0c6',
  },
  // June — rainy blue-violet
  {
    name: 'June',
    backdrop: ['#8a92c8', '#4d5390', '#1a1c3d'],
    wave: '#d5d9f5',
    glow: '#c6cbf7',
  },
  // July — bright ocean
  {
    name: 'July',
    backdrop: ['#6fbdd6', '#2f7c9b', '#0c2c3f'],
    wave: '#c8ecf7',
    glow: '#a8e4f7',
  },
  // August — deep turquoise
  {
    name: 'August',
    backdrop: ['#4fb5b0', '#1f7a78', '#082b2c'],
    wave: '#bdeeec',
    glow: '#9ceae6',
  },
  // September — purple
  {
    name: 'September',
    backdrop: ['#a186c4', '#5f4a8c', '#221838'],
    wave: '#e0d3f2',
    glow: '#d2bef7',
  },
  // October — orange
  {
    name: 'October',
    backdrop: ['#d9995a', '#a15c22', '#3d1f08'],
    wave: '#f7ddc0',
    glow: '#ffcf99',
  },
  // November — late-autumn amber
  {
    name: 'November',
    backdrop: ['#b5794f', '#7a4526', '#2e160b'],
    wave: '#eed6c2',
    glow: '#f0c3a0',
  },
  // December — midwinter blue
  {
    name: 'December',
    backdrop: ['#5f7fb5', '#2f4a80', '#0d1730'],
    wave: '#d3e0f5',
    glow: '#bcd4f7',
  },
];

/**
 * Theme for a given date, defaulting to now.
 *
 * `getMonth()` is zero-based, which lines up with the array directly.
 */
export function themeForDate(date: Date = new Date()): XmbTheme {
  return THEMES[date.getMonth()];
}

export function themeByName(name: string): XmbTheme | undefined {
  return THEMES.find((t) => t.name.toLowerCase() === name.toLowerCase());
}

export function allThemes(): readonly XmbTheme[] {
  return THEMES;
}

/** Pushes a theme into the CSS custom properties the stylesheet reads. */
export function applyTheme(theme: XmbTheme, root: HTMLElement): void {
  root.style.setProperty('--xmb-backdrop-top', theme.backdrop[0]);
  root.style.setProperty('--xmb-backdrop-mid', theme.backdrop[1]);
  root.style.setProperty('--xmb-backdrop-bottom', theme.backdrop[2]);
  root.style.setProperty('--xmb-wave', theme.wave);
  root.style.setProperty('--xmb-glow', theme.glow);
}
