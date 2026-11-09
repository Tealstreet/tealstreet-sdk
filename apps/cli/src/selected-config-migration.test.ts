import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

vi.setConfig({ testTimeout: 30_000 });
let root: string;
let outside: string;
beforeEach(() => {
  root = realpathSync(mkdtempSync(join(tmpdir(), 'cli-owner-selected-')));
  outside = mkdtempSync(join(tmpdir(), 'cli-owner-selected-outside-'));
});
afterEach(() => {
  rmSync(root, { recursive: true, force: true });
  rmSync(outside, { recursive: true, force: true });
});
function ownConfig(file: string, owner: string) {
  writeFileSync(
    file,
    JSON.stringify({
      accounts: [{ name: owner, exchange: 'bybit', apiKey: 'synthetic', apiSecret: 'synthetic' }],
      defaultAccount: owner,
      auth: { userId: owner, accessToken: 'synthetic' },
    }),
  );
}
function sharedHook() {
  writeFileSync(
    join(root, 'hooks.json'),
    JSON.stringify({
      hooks: [{ id: 'owned', name: 'owned-hook', secret: 'synthetic', command: 'synthetic', createdAt: 1 }],
    }),
  );
}
function run(file = 'selected.conf', mode = 'inspect') {
  return execFileSync('bun', [join(import.meta.dirname, 'selected-config-migration.fixture.ts'), mode], {
    env: {
      ...process.env,
      TEALSTREET_BRAND: 'tealstreet',
      TEALSTREET_CONFIG_DIR: root,
      TEALSTREET_CONFIG_FILE: join(root, file),
    },
    encoding: 'utf8',
    timeout: 20_000,
  });
}
function expectOwned(result: string) {
  expect(result).toContain('"accounts":["A"]');
  expect(result).toContain('"hasAuth":true');
  expect(result).toContain('"defaultAccount":"A"');
  expect(result).toContain('"requiresReentry":false');
  expect(result).toContain('"migrationMarker":true');
  expect(result).toContain('"inheritedHook":true');
}
function expectQuarantined(result: string) {
  expect(result).toContain('"accounts":[]');
  expect(result).toContain('"hasAuth":false');
  expect(result).toContain('"requiresReentry":true');
  expect(result).toContain('"migrationMarker":false');
  expect(result).toContain('"inheritedHook":false');
}

it('preserves a directly selected owned config', () => {
  ownConfig(join(root, 'A.json'), 'A');
  sharedHook();
  expectOwned(run('A.json'));
});

it('leaves a completed direct-file migration independent of later legacy config changes', () => {
  ownConfig(join(root, 'A.json'), 'A');
  sharedHook();
  expectOwned(run('A.json'));
  ownConfig(join(root, 'A.json'), 'B');
  expectOwned(run('A.json'));
});

it.each(['relative', 'absolute', 'nested'])(
  'preserves accounts, auth and defaults from an owned selected symlink: %s',
  (kind) => {
    ownConfig(join(root, 'A.json'), 'A');
    sharedHook();
    if (kind === 'nested') {
      mkdirSync(join(root, 'links'));
      symlinkSync('../A.json', join(root, 'links', 'config'));
    }
    const target = kind === 'absolute' ? join(root, 'A.json') : kind === 'nested' ? 'links/config' : 'A.json';
    symlinkSync(target, join(root, 'selected.conf'));
    expectOwned(run());
    // A second process must not see the old empty-profile/completed-marker state.
    expectOwned(run());
  },
);

it.each(['competing', 'external', 'broken', 'loop'])(
  'quarantines an unsafe selected symlink and warns without a success marker: %s',
  (kind) => {
    ownConfig(join(root, 'A.json'), 'A');
    sharedHook();
    if (kind === 'competing') ownConfig(join(root, 'B.conf'), 'B');
    if (kind === 'external') ownConfig(join(outside, 'A.json'), 'A');
    const target =
      kind === 'competing'
        ? 'B.conf'
        : kind === 'external'
          ? join(outside, 'A.json')
          : kind === 'broken'
            ? 'missing'
            : 'selected.conf';
    symlinkSync(target, join(root, 'selected.conf'));
    expectQuarantined(run());
    expectQuarantined(run());
  },
);

it('warns about a previously skipped selected config despite its old completion marker', () => {
  ownConfig(join(root, 'A.json'), 'A');
  symlinkSync('A.json', join(root, 'selected.conf'));
  sharedHook();
  run('selected.conf', 'simulate-skipped-config');
  const result = run();
  expect(result).toContain('"accounts":[]');
  expect(result).toContain('"hasAuth":false');
  expect(result).toContain('"requiresReentry":true');
});
