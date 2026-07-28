/**
 * Landing page behaviour: the wave backdrop and the download list.
 *
 * Plain ES modules with no build step, so Base44 can serve this directory as-is.
 */

const REPO_URL = 'https://github.com/ariberko/PSP-Emulator';

/**
 * Where the releases function lives.
 *
 * Base44 exposes deployed functions under `/api/functions/<name>` on the app's
 * own origin, so a relative URL works once this page is deployed alongside them
 * and needs no build-time configuration.
 */
const RELEASES_URL = '/api/functions/releases';

const PLATFORM_LABELS = {
  windows: { name: 'Windows', hint: '10 or later · installer' },
  'macos-arm': { name: 'macOS (Apple silicon)', hint: 'M1 and later · .dmg' },
  'macos-intel': { name: 'macOS (Intel)', hint: '.dmg' },
  'linux-appimage': { name: 'Linux', hint: 'AppImage · portable' },
  'linux-deb': { name: 'Linux', hint: '.deb · Debian and Ubuntu' },
};

// --- Wave backdrop ---------------------------------------------------------

/**
 * The same summed-sine ribbons the shell draws, slowed down.
 *
 * A page background should not compete with the content, so the amplitudes are
 * gentler and the whole canvas is held back with CSS opacity.
 */
function startWave(canvas) {
  const ctx = canvas.getContext('2d');
  if (!ctx) {
    return;
  }

  const ribbons = [
    { base: 0.52, amplitude: 0.05, wavelength: 0.9, speed: 0.16, phase: 0, thickness: 0.14, alpha: 0.2 },
    { base: 0.46, amplitude: 0.07, wavelength: 0.55, speed: -0.12, phase: 1.7, thickness: 0.09, alpha: 0.22 },
    { base: 0.6, amplitude: 0.045, wavelength: 0.38, speed: 0.21, phase: 3.1, thickness: 0.05, alpha: 0.24 },
    { base: 0.68, amplitude: 0.08, wavelength: 1.1, speed: -0.08, phase: 4.6, thickness: 0.2, alpha: 0.14 },
  ];

  let width = 0;
  let height = 0;

  const resize = () => {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    width = window.innerWidth;
    height = window.innerHeight;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  };

  const draw = (now) => {
    const seconds = now / 1000;
    ctx.clearRect(0, 0, width, height);
    ctx.globalCompositeOperation = 'lighter';

    for (const ribbon of ribbons) {
      const t = seconds * ribbon.speed + ribbon.phase;
      const centre = height * ribbon.base;
      const amplitude = height * ribbon.amplitude;
      const thickness = height * ribbon.thickness;
      const wavelength = width * ribbon.wavelength;

      const top = [];
      const bottom = [];
      for (let x = -20; x <= width + 20; x += 14) {
        const primary = Math.sin((x / wavelength) * Math.PI * 2 + t);
        const secondary = Math.sin((x / (wavelength * 0.41)) * Math.PI * 2 - t * 1.3);
        const y = centre + primary * amplitude + secondary * amplitude * 0.28;
        top.push([x, y - thickness / 2]);
        bottom.push([x, y + thickness / 2]);
      }

      const gradient = ctx.createLinearGradient(0, centre - thickness, 0, centre + thickness);
      gradient.addColorStop(0, 'rgba(168, 228, 247, 0)');
      gradient.addColorStop(0.5, `rgba(168, 228, 247, ${ribbon.alpha})`);
      gradient.addColorStop(1, 'rgba(168, 228, 247, 0)');

      ctx.beginPath();
      top.forEach(([x, y], i) => (i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)));
      for (let i = bottom.length - 1; i >= 0; i--) {
        ctx.lineTo(bottom[i][0], bottom[i][1]);
      }
      ctx.closePath();
      ctx.fillStyle = gradient;
      ctx.fill();
    }

    requestAnimationFrame(draw);
  };

  resize();
  window.addEventListener('resize', resize);
  requestAnimationFrame(draw);
}

// --- Downloads -------------------------------------------------------------

/**
 * Best guess at the visitor's platform, used to highlight one download.
 *
 * Only ever a hint: every platform stays listed, because sniffing is unreliable
 * and Apple silicon in particular is not always distinguishable.
 */
function guessPlatform() {
  const ua = navigator.userAgent;
  const platform = navigator.userAgentData?.platform ?? navigator.platform ?? '';

  if (/Win/i.test(platform) || /Windows/i.test(ua)) {
    return 'windows';
  }
  if (/Mac/i.test(platform) || /Mac OS X/i.test(ua)) {
    // Apple silicon is the safer default now, and the Intel build is right there.
    return 'macos-arm';
  }
  if (/Linux|X11/i.test(platform) || /Linux/i.test(ua)) {
    return 'linux-appimage';
  }
  return null;
}

function formatSize(bytes) {
  if (!bytes || bytes <= 0) {
    return null;
  }
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(value < 10 && unit > 0 ? 1 : 0)} ${units[unit]}`;
}

function renderDownloads(data) {
  const grid = document.getElementById('download-grid');
  const versionLine = document.getElementById('release-version');
  const platforms = data?.platforms ?? {};
  const entries = Object.entries(platforms);

  if (entries.length === 0) {
    versionLine.textContent = 'No builds have been published yet.';
    grid.innerHTML = `
      <div class="download-empty">
        <p>No releases yet — the first build is on its way.</p>
        <p><a href="${REPO_URL}">Build it from source</a> in the meantime.</p>
      </div>`;
    return;
  }

  versionLine.textContent = data.latest_version
    ? `Latest release: v${data.latest_version}`
    : 'Latest release';

  const preferred = guessPlatform();

  // Sort so the visitor's likely platform comes first.
  entries.sort(([a], [b]) => {
    if (a === preferred) return -1;
    if (b === preferred) return 1;
    return a.localeCompare(b);
  });

  grid.innerHTML = entries
    .map(([key, release]) => {
      const label = PLATFORM_LABELS[key] ?? { name: key, hint: '' };
      const size = formatSize(release.size_bytes);
      const meta = [label.hint, size].filter(Boolean).join(' · ');
      return `
        <a class="download-card" href="${escapeAttribute(release.download_url)}">
          <span class="platform">${escapeHtml(label.name)}</span>
          <span class="meta">v${escapeHtml(release.version)}${meta ? ` · ${escapeHtml(meta)}` : ''}</span>
        </a>`;
    })
    .join('');

  // Point the hero button at the visitor's platform when we have a guess.
  const heroButton = document.getElementById('primary-download');
  const heroLabel = document.getElementById('primary-download-label');
  const heroMeta = document.getElementById('primary-download-meta');
  const [topKey, topRelease] = entries[0];
  const topLabel = PLATFORM_LABELS[topKey] ?? { name: topKey };

  heroButton.href = topRelease.download_url;
  heroLabel.textContent = `Download for ${topLabel.name}`;
  heroMeta.textContent = `v${topRelease.version}${
    formatSize(topRelease.size_bytes) ? ` · ${formatSize(topRelease.size_bytes)}` : ''
  }`;
}

/** Shown when the function is unreachable — never a silently empty page. */
function renderDownloadError(reason) {
  const grid = document.getElementById('download-grid');
  document.getElementById('release-version').textContent =
    'Could not reach the release service.';
  grid.innerHTML = `
    <div class="download-empty">
      <p>${escapeHtml(reason)}</p>
      <p><a href="${REPO_URL}/releases">Check GitHub Releases directly</a>.</p>
    </div>`;
}

async function loadDownloads() {
  try {
    const response = await fetch(RELEASES_URL, { headers: { Accept: 'application/json' } });
    if (!response.ok) {
      throw new Error(`the service replied ${response.status}`);
    }
    renderDownloads(await response.json());
  } catch (error) {
    renderDownloadError(error instanceof Error ? error.message : String(error));
  }
}

// --- Helpers ---------------------------------------------------------------

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function escapeAttribute(value) {
  return escapeHtml(value).replace(/'/g, '&#39;');
}

// --- Start ----------------------------------------------------------------

const canvas = document.querySelector('.backdrop');
// Respect a reduced-motion preference: the CSS hides the canvas, so animating it
// would only burn battery.
if (canvas && !window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
  startWave(canvas);
}

for (const link of document.querySelectorAll('#repo-link')) {
  link.href = REPO_URL;
}

void loadDownloads();
