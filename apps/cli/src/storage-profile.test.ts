import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
// These tests start multiple real Bun processes; allow cold imports under load.
vi.setConfig({ testTimeout: 30_000 });
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
function sharedFiles(directory = root) {
  writeFileSync(
    join(directory, 'hooks.json'),
    JSON.stringify({
      hooks: [{ id: 'A', name: 'A-private', secret: 'synthetic', command: 'synthetic', createdAt: 1 }],
    }),
  );
  writeFileSync(
    join(directory, 'trusted-clients.json'),
    JSON.stringify({
      clients: [{ clientId: 'A-browser', label: 'A', origin: 'https://v3.tealstreet.io', approvedAt: 1 }],
    }),
  );
  writeFileSync(join(directory, 'vars.json'), JSON.stringify({ 'A-private': 'synthetic' }));
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
it.each(['B.conf', 'B'])('recognizes competing config ownership regardless of extension: %s', (file) => {
  ownConfig('A.json', 'A');
  ownConfig(file, 'B');
  sharedFiles();
  expectSharedAbsent(run('A.json'));
});
it.each(['inside', 'outside', 'broken', 'loop'])(
  'quarantines competing or unprovable symlink ownership: %s',
  (kind) => {
    ownConfig('A.json', 'A');
    const directory = join(root, kind === 'outside' ? 'outside' : 'configs');
    mkdirSync(directory);
    writeFileSync(join(directory, 'B.conf'), JSON.stringify({ accounts: [], auth: { userId: 'B' } }));
    if (kind === 'outside') {
      const source = join(root, 'source');
      mkdirSync(source);
      ownConfig('source/A.json', 'A');
      symlinkSync(join(directory, 'B.conf'), join(source, 'B.json'));
      sharedFiles(source);
      expectSharedAbsent(run('A.json', 'inspect', source));
      return;
    }
    const target = kind === 'broken' ? 'missing' : kind === 'loop' ? 'B.json' : join('configs', 'B.conf');
    symlinkSync(target, join(root, 'B.json'));
    sharedFiles();
    expectSharedAbsent(run('A.json'));
  },
);
it('allows an in-root symlink to a config with the same proven owner', () => {
  ownConfig('A.json', 'A');
  symlinkSync('A.json', join(root, 'same-owner.conf'));
  sharedFiles();
  expect(run('A.json')).toContain('"inheritedAHook":true');
});
it.each(['A', 'B'])('resolves nested in-root links and parent segments before deciding ownership: %s', (owner) => {
  ownConfig('A.json', 'A');
  const nested = join(root, 'nested');
  mkdirSync(nested);
  mkdirSync(join(nested, 'child'));
  ownConfig('nested/B', owner);
  symlinkSync('child', join(nested, 'alias'), 'dir');
  symlinkSync('nested/alias/../B', join(root, 'B.conf'));
  sharedFiles();
  const result = run('A.json');
  if (owner === 'A') expect(result).toContain('"inheritedAHook":true');
  else expectSharedAbsent(result);
});
it('ignores unrelated non-config files when ownership is otherwise proven', () => {
  ownConfig('A.json', 'A');
  writeFileSync(join(root, 'notes.conf'), JSON.stringify({ note: 'not a config' }));
  writeFileSync(join(root, 'cli.log'), 'synthetic log line');
  sharedFiles();
  expect(run('A.json')).toContain('"inheritedAHook":true');
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
