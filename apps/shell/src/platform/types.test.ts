import { describe, expect, it } from 'vitest';

import { describeInstall, formatSize, gameSublabel, type Game, type InstallReport } from './types';

function report(patch: Partial<InstallReport> = {}): InstallReport {
  return {
    target: '/home/me/.local/share/psp-emulator/Games',
    installed: [],
    already_present: [],
    failed: [],
    bytes_copied: 0,
    ...patch,
  };
}

describe('describeInstall', () => {
  it('names the count and size of a fresh install', () => {
    const line = describeInstall(
      report({ installed: ['Batman.pbp', 'Homebrew.iso'], bytes_copied: 12_582_912 }),
    );
    expect(line).toBe('Installed 2 games, 12 MB');
  });

  it('uses the singular for one game', () => {
    expect(describeInstall(report({ installed: ['Batman.pbp'], bytes_copied: 1024 }))).toBe(
      'Installed 1 game, 1.0 KB',
    );
  });

  it('distinguishes "already there" from "nothing to install"', () => {
    // The two produce identical numbers — zero copied — and conflating them is how
    // a working second click reads as a broken one.
    expect(describeInstall(report({ already_present: ['Batman.pbp'] }))).toBe(
      'Already installed — 1 game in place',
    );
    expect(describeInstall(report())).toBe('This build ships no games');
  });

  it('reports a failure over a partial success, with the reason', () => {
    // Saying "installed 1 game" while another failed silently is the worst
    // possible summary, so a failure always wins the line.
    const line = describeInstall(
      report({
        installed: ['Homebrew.iso'],
        failed: [{ file_name: 'Batman.pbp', reason: 'Permission denied (os error 13)' }],
      }),
    );
    expect(line).toBe('Batman.pbp failed: Permission denied (os error 13)');
  });

  it('counts the remaining failures when several went wrong', () => {
    const line = describeInstall(
      report({
        failed: [
          { file_name: 'A.pbp', reason: 'No space left on device' },
          { file_name: 'B.pbp', reason: 'No space left on device' },
          { file_name: 'C.pbp', reason: 'No space left on device' },
        ],
      }),
    );
    expect(line).toBe('A.pbp failed: No space left on device (and 2 more)');
  });

  it('omits the size when nothing measurable was copied', () => {
    // A zero-byte file is legal; "Installed 1 game, —" would be nonsense.
    expect(describeInstall(report({ installed: ['Empty.pbp'], bytes_copied: 0 }))).toBe(
      'Installed 1 game',
    );
  });
});

describe('formatSize', () => {
  it('keeps a decimal below ten so units do not collapse', () => {
    expect(formatSize(1_500_000_000)).toBe('1.4 GB');
    expect(formatSize(15_000_000_000)).toBe('14 GB');
  });

  it('shows bytes without a decimal', () => {
    expect(formatSize(512)).toBe('512 B');
  });

  it('renders an unknown or empty size as a dash', () => {
    expect(formatSize(0)).toBe('—');
    expect(formatSize(-1)).toBe('—');
  });
});

describe('gameSublabel', () => {
  const game: Game = {
    id: 'ULUS10041',
    title: 'Homebrew',
    path: '/games/homebrew.pbp',
    format: 'pbp',
    size_bytes: 68_157_440,
    disc_id: 'ULUS10041',
    disc_version: '1.00',
    category: 'MG',
    system_version: '6.60',
    parental_level: null,
    icon: null,
    background: null,
  };

  it('reads format, size and disc id', () => {
    expect(gameSublabel(game)).toBe('PBP  ·  65 MB  ·  ULUS10041');
  });

  it('drops the disc id when the file has none', () => {
    expect(gameSublabel({ ...game, disc_id: null })).toBe('PBP  ·  65 MB');
  });
});
