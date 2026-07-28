/**
 * Release manifest.
 *
 * Serves two clients:
 *
 * - `GET /releases` — the download page. Returns the newest non-prerelease,
 *   non-yanked build for every platform.
 * - `GET /releases?platform=…&version=…` — the desktop app's update check.
 *   Returns whether a newer build exists, and the artifact if so.
 *
 * Reads run under the service role because a release manifest is public
 * information: someone downloading the app for the first time has no account, and
 * the running app should not have to authenticate to learn it is out of date.
 * Nothing here writes — the release pipeline populates the entity — so elevated
 * reads cannot be turned into a way to publish anything.
 */

import { createClientFromRequest } from 'npm:@base44/sdk';

/** Matches the `platform` enum on the Release entity. */
const PLATFORMS = [
  'windows',
  'macos-intel',
  'macos-arm',
  'linux-appimage',
  'linux-deb',
] as const;

type Platform = (typeof PLATFORMS)[number];

interface Release {
  id: string;
  version: string;
  platform: Platform;
  download_url: string;
  size_bytes?: number;
  sha256?: string;
  release_notes?: string;
  published_at: string;
  prerelease?: boolean;
  yanked?: boolean;
}

/** The download page is served from a different origin than the function. */
const CORS_HEADERS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'GET, OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type, Authorization',
};

export default async function handler(req: Request): Promise<Response> {
  if (req.method === 'OPTIONS') {
    return new Response(null, { status: 204, headers: CORS_HEADERS });
  }
  if (req.method !== 'GET') {
    return json({ error: 'Only GET is supported' }, 405);
  }

  const base44 = createClientFromRequest(req);
  const url = new URL(req.url);
  const platform = url.searchParams.get('platform');
  const currentVersion = url.searchParams.get('version');

  if (platform && !isPlatform(platform)) {
    return json({ error: `Unknown platform "${platform}"`, platforms: PLATFORMS }, 400);
  }

  let releases: Release[];
  try {
    releases = (await base44.asServiceRole.entities.Release.list()) as Release[];
  } catch (error) {
    // Surface the failure rather than pretending there are no releases — the
    // download page would otherwise silently show nothing to download.
    return json({ error: `Could not read releases: ${describe(error)}` }, 502);
  }

  const available = releases.filter((r) => !r.yanked && !r.prerelease);

  // Update check: is there something newer for this platform?
  if (platform && currentVersion) {
    const latest = newestFor(available, platform);
    if (!latest) {
      return json({ update_available: false, reason: 'no release for this platform' });
    }
    const newer = compareVersions(latest.version, currentVersion) > 0;
    return json({
      update_available: newer,
      current_version: currentVersion,
      latest_version: latest.version,
      release: newer ? latest : null,
    });
  }

  // Download page: newest build per platform.
  const latest: Partial<Record<Platform, Release>> = {};
  for (const candidate of PLATFORMS) {
    const found = newestFor(available, candidate);
    if (found) {
      latest[candidate] = found;
    }
  }

  const newestVersion = Object.values(latest)
    .map((r) => r!.version)
    .sort(compareVersions)
    .pop();

  return json({ latest_version: newestVersion ?? null, platforms: latest });
}

function newestFor(releases: Release[], platform: Platform): Release | undefined {
  return releases
    .filter((r) => r.platform === platform)
    .sort((a, b) => compareVersions(a.version, b.version))
    .pop();
}

function isPlatform(value: string): value is Platform {
  return (PLATFORMS as readonly string[]).includes(value);
}

/**
 * Compares semver-ish versions numerically.
 *
 * A lexicographic compare would rank "0.10.0" below "0.9.0", which is exactly the
 * case an update check gets wrong first. Any pre-release suffix is ignored, since
 * pre-releases are filtered out before this is reached.
 */
export function compareVersions(a: string, b: string): number {
  const parse = (v: string) =>
    v
      .split('-')[0]
      .split('.')
      .map((part) => Number.parseInt(part, 10) || 0);

  const left = parse(a);
  const right = parse(b);
  const length = Math.max(left.length, right.length);

  for (let i = 0; i < length; i++) {
    const diff = (left[i] ?? 0) - (right[i] ?? 0);
    if (diff !== 0) {
      return diff > 0 ? 1 : -1;
    }
  }
  return 0;
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
  });
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
