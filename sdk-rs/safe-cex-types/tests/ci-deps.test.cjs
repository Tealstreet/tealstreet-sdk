const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {execFileSync} = require('node:child_process');
const {test, after} = require('node:test');

const root = path.resolve(__dirname, '../../..');
const target = fs.mkdtempSync(path.join(os.tmpdir(), 'safe-cex-model-oracle-deps-'));
after(() => fs.rmSync(target, {recursive: true, force: true}));
execFileSync(process.execPath, [path.resolve(__dirname, '../ci-deps.cjs'), target]);
const actual = JSON.parse(fs.readFileSync(path.join(target, 'package.json'), 'utf8'));
const venue = JSON.parse(fs.readFileSync(path.join(root, 'packages/safe-cex/package.json'), 'utf8'));
// These are the direct imports of the real Apex API/trading source oracles.
// A transitive package of another major cannot satisfy that dependency contract.
for (const name of ['@noble/hashes', 'bignumber.js', '@ethersproject/bignumber']) {
  test(`isolated model oracle declares the maintained ${name} selector`, () => {
    assert.equal(typeof venue.dependencies[name], 'string', `maintained venue selector ${name}`);
    assert.equal(actual.dependencies[name], venue.dependencies[name], `isolated bundle direct selector ${name}`);
  });
}
test('isolated model oracle keeps the repository lockfile bytes and package manager', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
  assert.equal(actual.packageManager, manifest.packageManager);
  assert.deepEqual(fs.readFileSync(path.join(target, 'yarn.lock')), fs.readFileSync(path.join(root, 'yarn.lock')));
});
