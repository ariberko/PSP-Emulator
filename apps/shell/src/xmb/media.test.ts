/**
 * Media helper tests.
 *
 * The important one here is the cross-check against the Rust scanner's extension
 * tables. If `psp-metadata` lists a format the shell has no MIME type for, that
 * file appears in the XMB and then fails to play on WebKit — a silent
 * inconsistency between two files in different languages, which is exactly the
 * kind of thing that only shows up on someone else's machine.
 */

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { mediaSublabel, mimeTypeFor } from './media';
import type { MediaItem } from '../platform/types';

const here = dirname(fileURLToPath(import.meta.url));
const MEDIA_RS = resolve(here, '../../../../crates/psp-metadata/src/media.rs');

/** Pulls one of the extension tables out of the Rust source. */
function rustExtensions(constant: string): string[] {
  const source = readFileSync(MEDIA_RS, 'utf8');
  const match = new RegExp(`${constant}:\\s*&\\[&str\\]\\s*=\\s*&\\[([^\\]]*)\\]`).exec(source);
  if (!match) {
    throw new Error(`could not find ${constant} in ${MEDIA_RS}`);
  }
  return [...match[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
}

function item(overrides: Partial<MediaItem> = {}): MediaItem {
  return {
    id: '/media/track.mp3',
    title: 'Track',
    path: '/media/track.mp3',
    kind: 'music',
    size_bytes: 0,
    extension: 'mp3',
    ...overrides,
  };
}

describe('MIME types', () => {
  it('covers every audio extension the Rust scanner accepts', () => {
    for (const ext of rustExtensions('MUSIC_EXTENSIONS')) {
      expect(mimeTypeFor(ext), `no MIME type for .${ext}`).toBeTruthy();
      expect(mimeTypeFor(ext)).toMatch(/^audio\//);
    }
  });

  it('covers every video extension the Rust scanner accepts', () => {
    for (const ext of rustExtensions('VIDEO_EXTENSIONS')) {
      expect(mimeTypeFor(ext), `no MIME type for .${ext}`).toBeTruthy();
      expect(mimeTypeFor(ext)).toMatch(/^video\//);
    }
  });

  it('is case insensitive', () => {
    expect(mimeTypeFor('MP3')).toBe('audio/mpeg');
    expect(mimeTypeFor('Mp4')).toBe('video/mp4');
  });

  it('returns nothing for an unknown extension, leaving the engine to guess', () => {
    expect(mimeTypeFor('xyz')).toBeUndefined();
  });

  it('declares opus with its codec, which some engines need', () => {
    expect(mimeTypeFor('opus')).toContain('codecs=opus');
  });
});

describe('the media sublabel', () => {
  it('shows the format in upper case', () => {
    expect(mediaSublabel(item({ extension: 'mp3' }))).toBe('MP3');
  });

  it('adds the size when known', () => {
    expect(mediaSublabel(item({ extension: 'wav', size_bytes: 88244 }))).toBe('WAV  ·  86 KB');
  });

  it('omits a zero size rather than showing "0 B"', () => {
    // Demo items in the browser build have no real size.
    expect(mediaSublabel(item({ size_bytes: 0 }))).toBe('MP3');
  });

  it('keeps a decimal for sub-10 values so 1.4 GB does not read as 1 GB', () => {
    expect(mediaSublabel(item({ extension: 'mp4', size_bytes: 1_503_238_553 }))).toBe(
      'MP4  ·  1.4 GB',
    );
  });
});
