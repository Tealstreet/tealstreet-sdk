import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { scannerExecutable } from './accepted-scanner.mjs';

function fixture() {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'scanner-receipt-'));
  const manifest = path.join(directory, 'Cargo.toml');
  const source = path.join(directory, 'main.rs');
  const executable = path.join(directory, 'actual-scanner');
  for (const file of [manifest, source, executable]) fs.writeFileSync(file, 'fixture');
  const target = { name: 'accepted-source', kind: ['bin'], src_path: source };
  const pkg = { name: 'safe-cex-parity', id: 'path+actual-sdk#safe-cex-parity@0.1.0', manifest_path: manifest, targets: [target] };
  const metadata = { packages: [pkg], workspace_members: [pkg.id], target_directory: '/unused-fixture-target' };
  const receipt = { reason: 'compiler-artifact', package_id: pkg.id, target, profile: { test: false }, executable, fresh: true };
  return { directory, manifest, source, executable, metadata, receipt };
}

test('scanner receipts bind the SDK package and actual fresh executable', () => {
  const f = fixture();
  try {
    assert.equal(scannerExecutable(f.metadata, JSON.stringify(f.receipt), f.manifest), fs.realpathSync(f.executable));
  } finally { fs.rmSync(f.directory, { recursive: true, force: true }); }
});

test('scanner receipt ambiguity missing files and foreign identities fail closed', () => {
  const f = fixture();
  const check = (metadata, receipts) => scannerExecutable(metadata, receipts.map(row => JSON.stringify(row)).join('\n'), f.manifest);
  try {
    for (const mutate of [
      row => { row.package_id = 'foreign-package'; },
      row => { row.target.name = 'unrelated'; },
      row => { row.target.kind = ['lib']; },
      row => { row.target.src_path = f.manifest; },
      row => { row.profile.test = true; },
      row => { row.executable = null; },
      row => { row.executable = path.join(f.directory, 'absent'); },
      row => { row.executable = f.directory; },
    ]) {
      const changed = structuredClone(f.receipt); mutate(changed);
      assert.throws(() => check(f.metadata, [changed]), /scanner artifact assertion/);
    }
    assert.throws(() => check(f.metadata, []), /one matching host compiler artifact/);
    assert.throws(() => check(f.metadata, [f.receipt, f.receipt]), /one matching host compiler artifact/);
    assert.throws(() => check({ ...f.metadata, workspace_members: [] }, [f.receipt]), /SDK workspace member/);
    assert.throws(() => check({ ...f.metadata, packages: [f.metadata.packages[0], f.metadata.packages[0]] }, [f.receipt]), /one scanner package manifest/);
    assert.throws(() => scannerExecutable(f.metadata, 'not JSON', f.manifest), /Cargo output must contain JSON/);
  } finally { fs.rmSync(f.directory, { recursive: true, force: true }); }
});

test('module alias local and birth scans reuse artifact-selected scanners', () => {
  const runner = fs.readFileSync(new URL('./accepted-set.mjs', import.meta.url), 'utf8');
  const births = fs.readFileSync(new URL('./accepted-births.mjs', import.meta.url), 'utf8');
  assert.equal((runner.match(/const scanner = buildScanner\(jobs\);/g) ?? []).length, 1);
  assert.equal((runner.match(/spawnSync\(scanner,/g) ?? []).length, 4);
  for (const mode of ['--doc-guards', '--binary-entry-json', '--alias-claims-json', '--macro-origins-json']) {
    assert.equal(runner.split(`spawnSync(scanner, ['${mode}'`).length - 1, 1, `${mode}: one artifact-selected scanner call`);
  }
  assert.match(births, /scanner \?\?= buildScanner\(\);/);
  for (const source of [runner, births]) assert.doesNotMatch(source, /target\/debug\/accepted-source/);
});
