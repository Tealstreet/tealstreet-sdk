import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const script = resolve(dirname(fileURLToPath(import.meta.url)), '../../../../scripts/check-eslint-sync.mjs');

test('root ESLint guard includes exact nested workspace paths and still rejects major drift', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'parity-workspace-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const writePackage = (path: string, manifest: object) => {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, JSON.stringify(manifest));
  };
  writePackage(join(root, 'package.json'), {
    private: true, resolutions: { eslint: '^9.28.0' }, workspaces: ['packages/*', 'sdk-rs/parity/extract-ts'],
  });
  writePackage(join(root, 'packages/example/package.json'), { devDependencies: { eslint: '^9.28.0' } });
  writePackage(join(root, 'vendored/package.json'), { devDependencies: { eslint: '^8.0.0' } });
  const nested = join(root, 'sdk-rs/parity/extract-ts/package.json');
  const run = () => spawnSync(process.execPath, [script], { cwd: root, encoding: 'utf8' });
  const absent = run();
  assert.equal(absent.status, 0, absent.stderr);
  assert.match(absent.stdout, /2 workspace package.json files/);
  writePackage(nested, { devDependencies: { eslint: '^9.28.0' } });
  const good = run();
  assert.equal(good.status, 0, good.stderr);
  assert.match(good.stdout, /3 workspace package.json files/);
  writePackage(nested, { devDependencies: { eslint: '^8.0.0' } });
  const bad = run();
  assert.equal(bad.status, 1);
  assert.match(bad.stderr, /sdk-rs\/parity\/extract-ts\/package.json: eslint=\^8.0.0/);
});
