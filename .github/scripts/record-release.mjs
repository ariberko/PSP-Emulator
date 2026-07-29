/**
 * Records a published release in Base44's `Release` entity.
 *
 * Run by the release workflow after the GitHub Release exists, so the download
 * page and the desktop app's update check can see the new build. Reads the
 * artifacts and digests from `dist/`.
 *
 * Deliberately best-effort about the network but strict about the data: a
 * malformed row is worse than a missing one, because the update check would offer
 * users a download that does not exist.
 *
 *   BASE44_API_KEY   workspace API key (b44k_…)
 *   BASE44_APP_ID    target app
 *   TAG              the tag being released, e.g. v0.2.0
 *   GITHUB_REPOSITORY  owner/repo, supplied by Actions
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

const API_KEY = process.env.BASE44_API_KEY;
const APP_ID = process.env.BASE44_APP_ID;
const TAG = process.env.TAG ?? '';
const REPO = process.env.GITHUB_REPOSITORY ?? '';
const SERVER = process.env.BASE44_API_URL ?? 'https://app.base44.com';
const DIST = 'dist';

/**
 * Maps an artifact filename to the `platform` enum on the Release entity.
 *
 * Order matters: the macOS checks must precede the generic ones, and the
 * architecture is only discoverable from the filename Tauri produced.
 */
function platformFor(name) {
  const lower = name.toLowerCase();
  if (lower.endsWith('.appimage')) return 'linux-appimage';
  if (lower.endsWith('.deb')) return 'linux-deb';
  if (lower.endsWith('.exe') || lower.endsWith('.msi')) return 'windows';
  if (lower.endsWith('.dmg')) {
    if (lower.includes('aarch64') || lower.includes('arm64')) return 'macos-arm';
    if (lower.includes('x64') || lower.includes('x86_64')) return 'macos-intel';
    // Tauri does not always encode the architecture. Refusing is better than
    // guessing: a row on the wrong platform sends users the wrong download.
    return null;
  }
  return null;
}

function fail(message) {
  console.error(`record-release: ${message}`);
  process.exit(1);
}

if (!API_KEY) fail('BASE44_API_KEY is not set');
if (!APP_ID) fail('BASE44_APP_ID is not set');
if (!/^v?\d+\.\d+\.\d+/.test(TAG)) fail(`TAG "${TAG}" is not a version tag`);

const version = TAG.replace(/^v/, '');

// Digests come from the checksum file the workflow wrote, so what is recorded is
// exactly what was published.
const digests = new Map();
try {
  const text = readFileSync(join(DIST, 'SHA256SUMS.txt'), 'utf8');
  for (const line of text.split('\n')) {
    const match = /^([0-9a-f]{64})\s+\*?(.+)$/.exec(line.trim());
    if (match) {
      digests.set(match[2], match[1]);
    }
  }
} catch (error) {
  fail(`could not read ${DIST}/SHA256SUMS.txt: ${error.message}`);
}

/**
 * Assets that are deliberately not platform builds.
 *
 * The release also carries the bundled games as standalone files. They are not
 * installers and have no `platform`, so they must be skipped *silently* — warning
 * about them every release would train the reader to ignore the warning that
 * matters, which is an installer nobody recognised.
 */
const NOT_A_BUILD = /\.(iso|cso|pbp|elf|prx)$/i;

const rows = [];
const skipped = [];

for (const name of readdirSync(DIST)) {
  if (name === 'SHA256SUMS.txt') continue;
  if (NOT_A_BUILD.test(name)) continue;
  const platform = platformFor(name);
  if (!platform) {
    skipped.push(name);
    continue;
  }
  rows.push({
    version,
    platform,
    download_url: `https://github.com/${REPO}/releases/download/${TAG}/${encodeURIComponent(name)}`,
    size_bytes: statSync(join(DIST, name)).size,
    sha256: digests.get(name) ?? null,
    published_at: new Date().toISOString(),
    prerelease: /-(alpha|beta|rc)/i.test(TAG),
    yanked: false,
  });
}

// Never report success on an empty run: silence here looks identical to a
// working pipeline, and the download page would stay empty with no explanation.
if (rows.length === 0) {
  fail(`no recognisable artifacts in ${DIST}/ (saw: ${skipped.join(', ') || 'nothing'})`);
}
if (skipped.length > 0) {
  console.warn(`record-release: skipped unrecognised artifacts: ${skipped.join(', ')}`);
}

let failures = 0;
for (const row of rows) {
  const response = await fetch(`${SERVER}/api/apps/${APP_ID}/entities/Release`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      // A workspace API key authenticates as the workspace, not a user.
      api_key: API_KEY,
    },
    body: JSON.stringify(row),
  });

  if (response.ok) {
    console.log(`recorded ${row.platform} ${row.version}`);
    continue;
  }
  failures++;
  console.error(
    `record-release: ${row.platform} failed with ${response.status}: ${await response.text()}`,
  );
}

if (failures > 0) {
  fail(`${failures} of ${rows.length} rows failed to record`);
}
console.log(`record-release: recorded ${rows.length} rows for ${version}`);
