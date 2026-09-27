import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { cleanupStaleUpdate, swapInPlace, updatePaths } from './windows-update';

// `windows-update.ts` imports renameSync directly, so vi.spyOn on the fs
// namespace does not intercept it — the binding is already resolved. Mock the
// module instead, and keep it pass-through unless a test arms `failRenameAt`.
let renameCalls = 0;
let failRenameAt: number | null = null;
vi.mock('node:fs', async (importOriginal) => {
  const actual = await importOriginal<typeof import('node:fs')>();
  return {
    ...actual,
    renameSync: (from: string, to: string) => {
      renameCalls += 1;
      if (failRenameAt !== null && renameCalls === failRenameAt) {
        throw new Error('EPERM: operation not permitted, rename');
      }
      return actual.renameSync(from, to);
    },
  };
});

const scratch = () => mkdtempSync(join(tmpdir(), 'win-update-'));

/**
 * The swap runs against a user's only copy of the binary, so the invariant that
 * matters is not "the update applied" — it is "there is still a working
 * tealstreet.exe afterwards, whatever happened".
 */
beforeEach(() => {
  renameCalls = 0;
  failRenameAt = null;
});

describe('swapInPlace', () => {
  it('replaces the binary and parks the old one', () => {
    const dir = scratch();
    const current = join(dir, 'tealstreet.exe');
    writeFileSync(current, 'OLD');
    const { next, previous } = updatePaths(current);
    writeFileSync(next, 'NEW');

    swapInPlace(current, next);

    expect(readFileSync(current, 'utf8')).toBe('NEW');
    expect(readFileSync(previous, 'utf8')).toBe('OLD');
    expect(existsSync(next)).toBe(false);
  });

  it('overwrites a leftover .old from an earlier update', () => {
    const dir = scratch();
    const current = join(dir, 'tealstreet.exe');
    writeFileSync(current, 'OLD');
    const { next, previous } = updatePaths(current);
    writeFileSync(next, 'NEW');
    writeFileSync(previous, 'ANCIENT');

    // Windows rename does not overwrite, so a stale .old would otherwise
    // abort every future update.
    swapInPlace(current, next);

    expect(readFileSync(current, 'utf8')).toBe('NEW');
    expect(readFileSync(previous, 'utf8')).toBe('OLD');
  });

  it('refuses when the staged download is missing', () => {
    const dir = scratch();
    const current = join(dir, 'tealstreet.exe');
    writeFileSync(current, 'OLD');

    expect(() => swapInPlace(current, join(dir, 'nope.exe'))).toThrow(/Staged update is missing/);
    // The running binary must be untouched.
    expect(readFileSync(current, 'utf8')).toBe('OLD');
  });

  it('restores the original binary if the second rename fails', () => {
    const dir = scratch();
    const current = join(dir, 'tealstreet.exe');
    writeFileSync(current, 'OLD');
    const { next, previous } = updatePaths(current);
    writeFileSync(next, 'NEW');

    // Rename 1 parks the running binary; fail rename 2, the one that would put
    // the new binary in its place. Without the rollback the user is left with
    // no tealstreet.exe at all — the worst outcome this code can produce.
    failRenameAt = 2;

    expect(() => swapInPlace(current, next)).toThrow(/EPERM/);

    expect(existsSync(current)).toBe(true);
    expect(readFileSync(current, 'utf8')).toBe('OLD');
    expect(existsSync(previous)).toBe(false);
  });
});

describe('cleanupStaleUpdate', () => {
  const realPlatform = process.platform;
  const setPlatform = (platform: string) =>
    Object.defineProperty(process, 'platform', { value: platform, configurable: true });
  afterEach(() => setPlatform(realPlatform));

  it('removes the parked binary on the next launch', () => {
    const dir = scratch();
    const current = join(dir, 'tealstreet.exe');
    const { previous } = updatePaths(current);
    writeFileSync(current, 'NEW');
    writeFileSync(previous, 'OLD');

    setPlatform('win32');
    cleanupStaleUpdate(current);

    expect(existsSync(previous)).toBe(false);
    expect(existsSync(current)).toBe(true);
  });

  it('clears a .new left behind by an interrupted download', () => {
    const dir = scratch();
    const current = join(dir, 'tealstreet.exe');
    const { next } = updatePaths(current);
    writeFileSync(current, 'CURRENT');
    writeFileSync(next, 'HALF-DOWNLOADED');

    setPlatform('win32');
    cleanupStaleUpdate(current);

    expect(existsSync(next)).toBe(false);
    expect(readFileSync(current, 'utf8')).toBe('CURRENT');
  });

  it('does nothing off Windows', () => {
    const dir = scratch();
    const current = join(dir, 'tealstreet');
    const { previous } = updatePaths(current);
    writeFileSync(current, 'NEW');
    writeFileSync(previous, 'OLD');

    setPlatform('darwin');
    cleanupStaleUpdate(current);

    expect(existsSync(previous)).toBe(true);
  });

  it('never throws, even on a path that cannot exist', () => {
    setPlatform('win32');
    expect(() => cleanupStaleUpdate('\0://not-a-path/tealstreet.exe')).not.toThrow();
  });
});
