/**
 * The Photo viewer and the Music/Video player.
 *
 * On a PSP, selecting a photo fills the screen with it and ○ returns to the bar;
 * music keeps playing while you navigate away, with a now-playing strip visible.
 * This reproduces both behaviours, which is why photos and video take over the
 * screen while music deliberately does not.
 *
 * Media is loaded from a URL the host provides — Tauri's asset protocol on the
 * desktop — so a large video streams into the element instead of being copied
 * across IPC.
 */

import type { MediaItem } from '../platform/types';

/**
 * Turns a `MediaError` into something a user can act on.
 *
 * "Could not play X" alone leaves nobody any wiser; naming the failure
 * distinguishes a codec this platform lacks from a file that has gone missing.
 */
function describeMediaError(element: HTMLMediaElement): string {
  switch (element.error?.code) {
    case MediaError.MEDIA_ERR_ABORTED:
      return 'playback was cancelled';
    case MediaError.MEDIA_ERR_NETWORK:
      return 'the file could not be read';
    case MediaError.MEDIA_ERR_DECODE:
      return 'the file is corrupt or uses an unsupported codec';
    case MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED:
      return 'this platform cannot play that format';
    default:
      return element.error?.message || 'unknown error';
  }
}

/**
 * MIME types by extension.
 *
 * Chromium sniffs an unlabelled media stream, but WebKit does not: handed a URL
 * with no usable `Content-Type` it fails with `SRC_NOT_SUPPORTED` before decoding
 * anything. Since the desktop app serves files over Tauri's asset protocol —
 * where the type is inferred, not guaranteed — the type is declared explicitly on
 * a `<source>` child instead of relying on it. That matters on macOS as well as
 * Linux, both of which use WebKit.
 */
const MIME_TYPES: Record<string, string> = {
  // Audio
  mp3: 'audio/mpeg',
  m4a: 'audio/mp4',
  aac: 'audio/aac',
  wav: 'audio/wav',
  ogg: 'audio/ogg',
  oga: 'audio/ogg',
  opus: 'audio/ogg; codecs=opus',
  flac: 'audio/flac',
  // Video
  mp4: 'video/mp4',
  m4v: 'video/mp4',
  webm: 'video/webm',
  ogv: 'video/ogg',
  mov: 'video/quicktime',
};

/** Best-known MIME type for a file extension, or `undefined` to let the engine guess. */
export function mimeTypeFor(extension: string): string | undefined {
  return MIME_TYPES[extension.toLowerCase()];
}

/**
 * Attaches a source to a media element with its type declared where known.
 *
 * A `<source>` child rather than `element.src`, because that is the only place a
 * `type` hint can be given.
 */
function attachSource(element: HTMLMediaElement, url: string, extension: string): void {
  const mime = mimeTypeFor(extension);
  if (!mime) {
    // Nothing better to offer than the URL itself.
    element.src = url;
    return;
  }
  const source = document.createElement('source');
  source.src = url;
  source.type = mime;
  element.appendChild(source);
}

export type MediaState = 'idle' | 'viewing' | 'playing' | 'paused';

export interface MediaHandlers {
  /** Called when the overlay closes, so the shell can restore its own key handling. */
  onClosed?: () => void;
  /** Reported so the shell can surface a failure as a toast. */
  onError?: (message: string) => void;
}

export class MediaSurface {
  private readonly root: HTMLElement;
  private readonly handlers: MediaHandlers;
  private overlay: HTMLElement | null = null;
  private audio: HTMLAudioElement | null = null;
  /** The music track currently loaded, which outlives the overlay. */
  private nowPlaying: MediaItem | null = null;
  private nowPlayingStrip: HTMLElement | null = null;

  constructor(root: HTMLElement, handlers: MediaHandlers = {}) {
    this.root = root;
    this.handlers = handlers;
  }

  /** Whether a full-screen surface is up and owning input. */
  get isOpen(): boolean {
    return this.overlay !== null;
  }

  get isPlayingMusic(): boolean {
    return this.audio !== null && !this.audio.paused;
  }

  /** Fills the screen with a photo. */
  showPhoto(item: MediaItem, url: string): void {
    this.closeOverlay();
    const overlay = this.createOverlay('photo');
    const image = document.createElement('img');
    image.className = 'media-photo';
    image.alt = item.title;
    image.src = url;
    // A file the webview cannot decode must say so rather than showing a blank
    // screen the user has to guess their way out of.
    image.addEventListener('error', () => {
      this.handlers.onError?.(`Could not display ${item.title}`);
      this.closeOverlay();
    });
    overlay.appendChild(image);
    overlay.appendChild(this.caption(item.title, 'Press ○ to go back'));
    this.root.appendChild(overlay);
  }

  /** Fills the screen with a video and starts it. */
  playVideo(item: MediaItem, url: string): void {
    this.closeOverlay();
    const overlay = this.createOverlay('video');
    const video = document.createElement('video');
    video.className = 'media-video';
    video.autoplay = true;
    video.controls = false;
    video.addEventListener('error', () => {
      this.handlers.onError?.(`Could not play ${item.title} — ${describeMediaError(video)}`);
      this.closeOverlay();
    });
    // Returning to the bar when a video finishes matches the console.
    video.addEventListener('ended', () => this.closeOverlay());
    attachSource(video, url, item.extension);
    overlay.appendChild(video);
    overlay.appendChild(this.caption(item.title, '✕ pause  ·  ○ stop'));
    this.root.appendChild(overlay);
    void video.play().catch(() => {
      // Autoplay can be refused; the ✕ toggle still starts it.
    });
  }

  /**
   * Starts a track and shows the now-playing strip.
   *
   * No overlay: music continues while the XMB is navigated, as on hardware.
   */
  playMusic(item: MediaItem, url: string): void {
    this.stopMusic();

    const audio = new Audio();
    audio.addEventListener('error', () => {
      this.handlers.onError?.(`Could not play ${item.title} — ${describeMediaError(audio)}`);
      this.stopMusic();
    });
    audio.addEventListener('ended', () => this.stopMusic());
    attachSource(audio, url, item.extension);
    this.audio = audio;
    this.nowPlaying = item;
    void audio.play().catch(() => {
      this.handlers.onError?.(`Could not start ${item.title}`);
      this.stopMusic();
    });
    this.renderNowPlaying();
  }

  /** Toggles the current track or video, returning the resulting state. */
  togglePlayback(): MediaState {
    const video = this.overlay?.querySelector('video');
    if (video) {
      if (video.paused) {
        void video.play().catch(() => {});
        return 'playing';
      }
      video.pause();
      return 'paused';
    }

    if (!this.audio) {
      return 'idle';
    }
    if (this.audio.paused) {
      void this.audio.play().catch(() => {});
      this.renderNowPlaying();
      return 'playing';
    }
    this.audio.pause();
    this.renderNowPlaying();
    return 'paused';
  }

  stopMusic(): void {
    if (this.audio) {
      this.audio.pause();
      // Dropping the source releases the file handle, which matters on Windows
      // where an open handle can block other operations on it. The <source> child
      // has to go too, or load() would pick the old track straight back up.
      this.audio.removeAttribute('src');
      this.audio.replaceChildren();
      this.audio.load();
      this.audio = null;
    }
    this.nowPlaying = null;
    this.nowPlayingStrip?.remove();
    this.nowPlayingStrip = null;
  }

  /** Closes a photo or video surface. Music is unaffected. */
  closeOverlay(): void {
    if (!this.overlay) {
      return;
    }
    const video = this.overlay.querySelector('video');
    if (video) {
      video.pause();
      video.removeAttribute('src');
      video.replaceChildren();
      video.load();
    }
    this.overlay.remove();
    this.overlay = null;
    this.handlers.onClosed?.();
  }

  /** Tears everything down, for teardown or a hard reset. */
  dispose(): void {
    this.closeOverlay();
    this.stopMusic();
  }

  private createOverlay(kind: 'photo' | 'video'): HTMLElement {
    const overlay = document.createElement('div');
    overlay.className = `media-overlay media-overlay--${kind}`;
    this.overlay = overlay;
    return overlay;
  }

  private caption(title: string, hint: string): HTMLElement {
    const caption = document.createElement('div');
    caption.className = 'media-caption';
    caption.innerHTML = `
      <span class="media-caption-title"></span>
      <span class="media-caption-hint"></span>
    `;
    // textContent rather than innerHTML: a file name is untrusted input.
    caption.querySelector('.media-caption-title')!.textContent = title;
    caption.querySelector('.media-caption-hint')!.textContent = hint;
    return caption;
  }

  private renderNowPlaying(): void {
    if (!this.nowPlaying) {
      return;
    }
    if (!this.nowPlayingStrip) {
      this.nowPlayingStrip = document.createElement('div');
      this.nowPlayingStrip.className = 'media-now-playing';
      this.root.appendChild(this.nowPlayingStrip);
    }
    const paused = this.audio?.paused ?? true;
    this.nowPlayingStrip.innerHTML = `
      <span class="media-now-playing-icon">${paused ? '❙❙' : '▶'}</span>
      <span class="media-now-playing-title"></span>
    `;
    this.nowPlayingStrip.querySelector('.media-now-playing-title')!.textContent =
      this.nowPlaying.title;
  }
}

/** The line under a media item: format and size. */
export function mediaSublabel(item: MediaItem): string {
  const parts = [item.extension.toUpperCase()];
  if (item.size_bytes > 0) {
    parts.push(formatSize(item.size_bytes));
  }
  return parts.join('  ·  ');
}

function formatSize(bytes: number): string {
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(value < 10 && unit > 0 ? 1 : 0)} ${units[unit]}`;
}
