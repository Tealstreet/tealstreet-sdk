import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const prefix = 'accepted-set: scanner artifact assertion';
function canonical(file) {
  assert.ok(typeof file === 'string' && fs.existsSync(file), `${prefix}: missing file ${file}`);
  return fs.realpathSync(file);
}

export function scannerExecutable(metadata, output, packageManifest) {
  const expectedManifest = canonical(packageManifest);
  const packages = metadata.packages.filter(pkg => canonical(pkg.manifest_path) === expectedManifest);
  assert.equal(packages.length, 1, `${prefix}: expected one scanner package manifest`);
  const pkg = packages[0];
  assert.equal(pkg.name, 'safe-cex-parity', `${prefix}: unexpected scanner package`);
  assert.ok(metadata.workspace_members.includes(pkg.id), `${prefix}: scanner must be an SDK workspace member`);
  const targets = pkg.targets.filter(target => target.name === 'accepted-source' && target.kind.includes('bin'));
  assert.equal(targets.length, 1, `${prefix}: expected one scanner bin target`);
  const target = targets[0];
  const source = canonical(target.src_path);
  const messages = output.trim().split('\n').filter(Boolean).map(line => {
    try { return JSON.parse(line); }
    catch { assert.fail(`${prefix}: Cargo output must contain JSON records`); }
  });
  const receipts = messages.filter(item => item.reason === 'compiler-artifact'
    && item.package_id === pkg.id && item.target?.name === target.name
    && item.target.kind?.includes('bin') && canonical(item.target.src_path) === source
    && item.profile?.test === false);
  assert.equal(receipts.length, 1, `${prefix}: expected one matching host compiler artifact`);
  const receipt = receipts[0];
  assert.ok(typeof receipt.executable === 'string' && fs.existsSync(receipt.executable)
    && fs.statSync(receipt.executable).isFile(), `${prefix}: scanner executable must be a real file`);
  return fs.realpathSync(receipt.executable);
}

// Build from the SDK context, even when the audited workspace is a fixture.
// Force a host tool independently of the audited target or Cargo's target default.
export function buildScanner(jobLimit = 2) {
  const jobs = String(jobLimit);
  assert.match(jobs, /^[12]$/, `${prefix}: jobs must be 1 or 2`);
  const sdk = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
  function run(command, args) {
    const result = spawnSync(command, args, {
      cwd: sdk, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
      env: { ...process.env, CARGO_BUILD_JOBS: jobs },
    });
    assert.equal(result.status, 0, `${prefix}: ${command} failed\n${result.error ?? ''}\n${result.stdout}\n${result.stderr}`);
    return result.stdout;
  }
  const host = run('rustc', ['--version', '--verbose']).match(/^host: (\S+)$/m)?.[1];
  assert.ok(host, `${prefix}: rustc must identify its host`);
  const manifest = path.join(sdk, 'Cargo.toml');
  const metadata = JSON.parse(run('cargo', ['metadata', '--locked', '--no-deps', '--format-version', '1', '--manifest-path', manifest]));
  const output = run('cargo', ['build', '--locked', '--manifest-path', manifest, '-p', 'safe-cex-parity', '--bin', 'accepted-source', '--target', host, '--jobs', jobs, '--message-format=json']);
  return scannerExecutable(metadata, output, path.join(sdk, 'safe-cex-parity/Cargo.toml'));
}
