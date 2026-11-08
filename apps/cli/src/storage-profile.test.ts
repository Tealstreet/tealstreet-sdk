import { afterEach, beforeEach, expect, it } from 'vitest';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
let root: string;
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'cli-owner-source-'));
});
afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});
function run(file: string, mode = 'inspect', directory = root): string {
  return execFileSync('bun', [join(import.meta.dirname, 'storage-profile.fixture.ts'), mode], {
    env: {
      ...process.env,
      HOME: root,
      USERPROFILE: root,
      TEALSTREET_BRAND: 'tealstreet',
      TEALSTREET_CONFIG_DIR: directory,
      TEALSTREET_CONFIG_FILE: join(directory, file),
    },
    encoding: 'utf8',
    timeout: 20_000,
  });
}
function ownConfig(file: string, userId: string) {
  writeFileSync(
    join(root, file),
    JSON.stringify({
      accounts: [{ name: userId, exchange: 'bybit', apiKey: 'synthetic', apiSecret: 'synthetic' }],
      auth: { userId, accessToken: 'synthetic' },
    }),
  );
}
function sharedFiles() {
  writeFileSync(
    join(root, 'hooks.json'),
    JSON.stringify({
      hooks: [{ id: 'A', name: 'A-private', secret: 'synthetic', command: 'synthetic', createdAt: 1 }],
    }),
  );
  writeFileSync(
    join(root, 'trusted-clients.json'),
    JSON.stringify({
      clients: [{ clientId: 'A-browser', label: 'A', origin: 'https://v3.tealstreet.io', approvedAt: 1 }],
    }),
  );
  writeFileSync(join(root, 'vars.json'), JSON.stringify({ 'A-private': 'synthetic' }));
}
function expectSharedAbsent(result: string) {
  expect(result).toContain('"inheritedAHook":false');
  expect(result).toContain('"inheritedAApproval":false');
  expect(result).toContain('"inheritedAVar":false');
}
it('keeps separately owned configs but quarantines ambiguous shared legacy files', () => {
  ownConfig('A.json', 'A');
  ownConfig('B.json', 'B');
  sharedFiles();
  const a = run('A.json'),
    b = run('B.json');
  expect(a).toContain('"accounts":["A"]');
  expect(b).toContain('"accounts":["B"]');
  expectSharedAbsent(a);
  expectSharedAbsent(b);
});
it('migrates a uniquely owned source once, never to a later user/config or symlink alias', () => {
  ownConfig('A.json', 'A');
  sharedFiles();
  expect(run('A.json')).toContain('"inheritedAHook":true');
  ownConfig('B.json', 'B');
  expectSharedAbsent(run('B.json'));
  const alias = join(root, 'alias');
  symlinkSync(root, alias, 'dir');
  expectSharedAbsent(run('B.json', 'inspect', alias));
});
it('honors prior source ownership evidence when a new config replaces it', () => {
  const old = join(root, 'profiles', 'old-profile');
  mkdirSync(old, { recursive: true });
  writeFileSync(join(old, 'legacy-owner.json'), JSON.stringify({ owner: 'A' }));
  ownConfig('B.json', 'B');
  sharedFiles();
  expectSharedAbsent(run('B.json'));
});
it('persists signed-out local accounts/defaults/vars/aliases across processes without importing them on login', () => {
  expect(run('config.json', 'local-seed')).toContain('"accounts":["Local"]');
  const restored = run('config.json');
  expect(restored).toContain('"owner":null');
  expect(restored).toContain('"defaultAccount":"Local"');
  expect(restored).toContain('"accounts":["Local"]');
  expect(restored).toContain('"localVar":"synthetic-local"');
  expect(restored).toContain('"localAlias":"synthetic-local"');
  const loggedIn = run('config.json', 'login');
  expect(loggedIn).toContain('"accounts":[]');
  expect(loggedIn).not.toContain('"localVar"');
});

it('does not let a missing alternate config claim or strand a uniquely owned legacy source', () => {
  ownConfig('A.json', 'A');
  sharedFiles();
  expectSharedAbsent(run('missing.json'));
  expect(run('A.json')).toContain('"inheritedAHook":true');
});
