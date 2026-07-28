/**
 * Rasterises icon.svg to a 1024x1024 PNG for `tauri icon` to expand.
 *
 * Uses the Chromium that Playwright already provides rather than adding an image
 * library: the SVG uses gradients and opacity, and a browser renders those
 * exactly as designed.
 */
import { chromium } from 'playwright';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const source = resolve(here, '..', 'icon.svg');
const output = process.argv[2] ?? resolve(here, '..', 'icon.png');
const SIZE = 1024;

const svg = readFileSync(source, 'utf8');

const browser = await chromium.launch({
  // Set by the container image; falls back to Playwright's own lookup.
  executablePath: process.env.CHROMIUM_PATH || undefined,
});
const page = await browser.newPage({
  viewport: { width: SIZE, height: SIZE },
  deviceScaleFactor: 1,
});

// A bare SVG document, so nothing but the artwork ends up in the raster.
await page.setContent(
  `<!doctype html><style>
     html,body{margin:0;padding:0;background:transparent}
     svg{display:block;width:${SIZE}px;height:${SIZE}px}
   </style>${svg}`,
  { waitUntil: 'load' },
);

await page.screenshot({ path: output, omitBackground: true });
await browser.close();

console.log(`wrote ${output} (${SIZE}x${SIZE})`);
