import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

/**
 * ENFORCEMENT: there is one `CoreAdapter` implementation and it lives here.
 *
 * Three hosts each hand-rolled this interface — web 562 lines, CLI 485, mobile
 * 388 — and because nothing compared them they drifted where it mattered most:
 * the CLI applied no rounding and no minimum-size validation, only web ran the
 * reduce-only clamp, `getCombinedBalance([])` meant "none" on two hosts and
 * "all of them" on the third. Every one of those was a silent wrong number,
 * not a crash.
 *
 * Documentation did not prevent it and would not prevent it again. This does:
 * a file that BUILDS a `CoreAdapter` — rather than consuming one — has to get
 * it from `createCoreAdapter`. A host that wants different behaviour adds a
 * BINDING; it does not write a method.
 *
 * This covers `tealstreet-next`. The mobile repo is a second checkout with its
 * own copy of the rule — see the venn table in this package's CLAUDE.md.
 */

const REPO_ROOT = path.resolve(__dirname, '../../../..');
// The public SDK mirror ships no apps/web, so the host files these name are absent there.
// package.json, not the directory: the mirror's CI can leave a generated apps/web/ behind.
const HAS_WEB_APP = existsSync(path.join(REPO_ROOT, 'apps/web/package.json'));

/** Five method names that only an implementation carries as object keys.
 *  A caller holding an adapter references them as `adapter.getPositions(...)`,
 *  never as `getPositions:`. */
const ADAPTER_KEYS = [
  'getActiveAccount:',
  'getPositions:',
  'getOrders:',
  'placeOrder:',
  'closePosition:',
  'setLeverage:',
] as const;

/**
 * Exported shape rather than an inline regex so the detector itself can be
 * tested. A guard that silently matches nothing is worse than no guard.
 */
export const looksLikeHandRolledAdapter = (source: string): boolean => {
  if (/implements\s+CoreAdapter\b/.test(source)) return true;
  const declares = /:\s*CoreAdapter\b/.test(source);
  if (!declares) return false;
  const keys = ADAPTER_KEYS.filter((key) => source.includes(key)).length;
  return keys >= 4;
};

const isOwnPackage = (file: string): boolean => file.startsWith('packages/script-api-core/');

/** Test doubles are fine — a fake adapter in a test is not a second runtime. */
const isTest = (file: string): boolean => /\.test\.tsx?$/.test(file);

const trackedFiles = (): readonly string[] =>
  execFileSync('git', ['ls-files', '*.ts', '*.tsx'], { cwd: REPO_ROOT, encoding: 'utf8' }).split('\n').filter(Boolean);

describe('no hand-rolled CoreAdapters', () => {
  it('detects an implementation, and does NOT flag a consumer', () => {
    expect(
      looksLikeHandRolledAdapter(
        `const a: CoreAdapter = { getActiveAccount: () => '', getPositions: () => [],
          getOrders: () => [], placeOrder: async () => ({ orderIds: [] }) };`,
      ),
    ).toBe(true);
    expect(looksLikeHandRolledAdapter('class X implements CoreAdapter {}')).toBe(true);

    // The shapes that must NOT trip it: holding one, passing one, returning
    // the factory's result.
    expect(looksLikeHandRolledAdapter('const run = (adapter: CoreAdapter) => adapter.getPositions({});')).toBe(false);
    expect(looksLikeHandRolledAdapter('export const make = (): CoreAdapter => createCoreAdapter(bindings);')).toBe(
      false,
    );
  });

  // Walks the source tree; under CI load this outruns vitest's 5s default.
  it('no host in this repo implements the interface', () => {
    const offenders = trackedFiles()
      .filter((file) => !isOwnPackage(file) && !isTest(file))
      .filter((file) => looksLikeHandRolledAdapter(readFileSync(path.join(REPO_ROOT, file), 'utf8')));

    expect(
      offenders,
      'These build a CoreAdapter without createCoreAdapter. A runtime supplies ' +
        'HostBindings; it does not implement the interface. If a host needs ' +
        'different behaviour, add a binding in HostBindings.ts.',
    ).toEqual([]);
  }, 30_000);

  it.skipIf(!HAS_WEB_APP)('the two host spokes exist and go through the factory', () => {
    // Pins that the spokes are still spokes — if one is deleted or renamed the
    // guard above would pass for the wrong reason.
    for (const spoke of ['apps/web/src/components/customScripts/scriptAdapter.ts', 'apps/cli/src/script-adapter.ts']) {
      const source = readFileSync(path.join(REPO_ROOT, spoke), 'utf8');
      expect(source, `${spoke} should build its adapter from the shared body`).toMatch(/createCoreAdapter\s*\(/);
      expect(source, `${spoke} should supply HostBindings`).toMatch(/HostBindings/);
    }
  });
});
