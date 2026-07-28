/**
 * Category and item icons, drawn as inline SVG.
 *
 * Original artwork in the spirit of the XMB's icon set — Sony's actual icons are
 * copyrighted, so nothing here is traced from them. Each is authored in a 48×48
 * box and inherits `currentColor` so the stylesheet controls selected/unselected
 * brightness in one place.
 */

export type GlyphName =
  | 'settings'
  | 'photo'
  | 'music'
  | 'video'
  | 'game'
  | 'network'
  | 'umd'
  | 'memstick'
  | 'folder'
  | 'play'
  | 'refresh'
  | 'save'
  | 'info'
  | 'cloud'
  | 'controller';

const GLYPHS: Record<GlyphName, string> = {
  // Wrench crossed over a gear — the settings toolbox.
  settings: `
    <path d="M31 8a9 9 0 0 0-8.2 12.7L11 32.5a3.5 3.5 0 1 0 4.9 4.9l11.8-11.8A9 9 0 1 0 31 8Zm0 4a5 5 0 1 1 0 10 5 5 0 0 1 0-10Z" fill="currentColor"/>
    <circle cx="34" cy="34" r="6" fill="none" stroke="currentColor" stroke-width="2.6"/>
    <path d="M34 24v4M34 40v4M24 34h4M40 34h4" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"/>
  `,
  // Landscape in a frame.
  photo: `
    <rect x="7" y="11" width="34" height="26" rx="3" fill="none" stroke="currentColor" stroke-width="2.8"/>
    <circle cx="17" cy="20" r="3.4" fill="currentColor"/>
    <path d="M11 33l8.5-9 6 6.5L31 25l7 8Z" fill="currentColor"/>
  `,
  // Beamed eighth notes.
  music: `
    <path d="M22 9v20.5a6 6 0 1 1-3-5.2V13l17-3.6v18.9a6 6 0 1 1-3-5.2V13.9Z" fill="currentColor"/>
  `,
  // Film strip.
  video: `
    <rect x="8" y="12" width="32" height="24" rx="2.5" fill="none" stroke="currentColor" stroke-width="2.8"/>
    <path d="M8 18h5M8 24h5M8 30h5M35 18h5M35 24h5M35 30h5" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/>
    <path d="M21 19l9 5-9 5Z" fill="currentColor"/>
  `,
  // UMD: disc outline, hub, and one highlighted sector to suggest the label.
  game: `
    <circle cx="24" cy="24" r="16" fill="none" stroke="currentColor" stroke-width="3"/>
    <path d="M24 10a14 14 0 0 1 12.1 7L24 24Z" fill="currentColor" opacity="0.8"/>
    <circle cx="24" cy="24" r="5" fill="currentColor"/>
  `,
  // Radiating arcs over a base — network signal.
  network: `
    <circle cx="24" cy="34" r="3.6" fill="currentColor"/>
    <path d="M15.5 27.5a12 12 0 0 1 17 0" fill="none" stroke="currentColor" stroke-width="2.8" stroke-linecap="round"/>
    <path d="M10.5 21.5a19 19 0 0 1 27 0" fill="none" stroke="currentColor" stroke-width="2.8" stroke-linecap="round"/>
  `,
  umd: `
    <circle cx="24" cy="24" r="15" fill="none" stroke="currentColor" stroke-width="2.8"/>
    <circle cx="24" cy="24" r="4" fill="currentColor"/>
  `,
  // Memory Stick Duo.
  memstick: `
    <path d="M15 8h13l5 5v27a2 2 0 0 1-2 2H17a2 2 0 0 1-2-2V10a2 2 0 0 1 0-2Z" fill="none" stroke="currentColor" stroke-width="2.8"/>
    <path d="M20 15h8M20 20h8" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/>
  `,
  folder: `
    <path d="M8 13h12l3 4h17a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V15a2 2 0 0 1 2-2Z" fill="none" stroke="currentColor" stroke-width="2.8"/>
  `,
  play: `
    <path d="M18 12l18 12-18 12Z" fill="currentColor"/>
  `,
  refresh: `
    <path d="M38 24a14 14 0 1 1-4.6-10.4" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>
    <path d="M36 6v10h-10" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
  `,
  save: `
    <path d="M11 9h20l6 6v24a2 2 0 0 1-2 2H13a2 2 0 0 1-2-2V11a2 2 0 0 1 0-2Z" fill="none" stroke="currentColor" stroke-width="2.8"/>
    <rect x="17" y="9" width="12" height="9" fill="currentColor"/>
    <rect x="16" y="26" width="16" height="13" fill="none" stroke="currentColor" stroke-width="2.4"/>
  `,
  info: `
    <circle cx="24" cy="24" r="16" fill="none" stroke="currentColor" stroke-width="2.8"/>
    <path d="M24 21v12" stroke="currentColor" stroke-width="3.2" stroke-linecap="round"/>
    <circle cx="24" cy="15.5" r="2.2" fill="currentColor"/>
  `,
  // A gamepad silhouette: grips, D-pad and two face buttons.
  controller: `
    <path d="M15 17h18a9 9 0 0 1 8.6 6.4l2.4 8a5.5 5.5 0 0 1-10 4.4l-2.6-4.3a3 3 0 0 0-2.6-1.5h-10.6a3 3 0 0 0-2.6 1.5l-2.6 4.3a5.5 5.5 0 0 1-10-4.4l2.4-8A9 9 0 0 1 15 17Z" fill="none" stroke="currentColor" stroke-width="2.6"/>
    <path d="M15.5 22.5v6M12.5 25.5h6" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/>
    <circle cx="31" cy="24" r="2" fill="currentColor"/>
    <circle cx="35.5" cy="27.5" r="2" fill="currentColor"/>
  `,
  cloud: `
    <path d="M16 34a7 7 0 0 1-.6-14 11 11 0 0 1 20.7 3A6.5 6.5 0 0 1 34 34Z" fill="none" stroke="currentColor" stroke-width="2.8"/>
    <path d="M24 30V19m0 0-4.5 4.5M24 19l4.5 4.5" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>
  `,
};

export function isGlyphName(value: string): value is GlyphName {
  return value in GLYPHS;
}

/** Renders a glyph as a standalone SVG element string. */
export function glyphSvg(name: GlyphName, className = 'xmb-glyph'): string {
  return `<svg class="${className}" viewBox="0 0 48 48" aria-hidden="true">${GLYPHS[name]}</svg>`;
}

/**
 * Resolves an item's `icon` to markup.
 *
 * Games carry a `data:` URL extracted from their ICON0; everything else names a
 * built-in glyph. Anything unrecognised falls back to the UMD disc rather than
 * rendering an empty slot.
 */
export function iconMarkup(icon: string | undefined, fallback: GlyphName = 'umd'): string {
  if (icon && (icon.startsWith('data:') || icon.startsWith('http'))) {
    // Game icons are 144×80; the CSS box preserves that aspect.
    return `<img class="xmb-icon-image" src="${escapeAttribute(icon)}" alt="" />`;
  }
  if (icon && isGlyphName(icon)) {
    return glyphSvg(icon, 'xmb-glyph xmb-glyph--item');
  }
  return glyphSvg(fallback, 'xmb-glyph xmb-glyph--item');
}

function escapeAttribute(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
}
