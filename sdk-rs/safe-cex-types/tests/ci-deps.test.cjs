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
// Direct runtime dependencies exercised by the shared isolated oracle bundle.
// A transitive package of another major cannot satisfy that dependency contract.
for (const name of ['axios', 'qs', 'create-hmac', 'axios-retry', 'axios-rate-limit', 'nanoid', 'tiny-emitter',
  'bignumber.js', '@noble/hashes', '@ethersproject/bignumber', '@noble/ed25519', 'bs58',
  'lodash', 'p-iteration', 'dayjs', 'ethers']) {
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

// Read the real oracle entrypoints so a new external source import also needs
// an explicit direct selector, including package-root normalization for subpaths.
for (const file of ['backpack/backpack.signing.ts', 'orderly/orderly.encryption.ts', 'binance/binanceinverse.exchange.ts']) {
  test(`isolated oracle covers every external source import in ${file}`, () => {
    const ts = require('typescript');
    const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges', file), 'utf8');
    const imports = ts.preProcessFile(source).importedFiles.map(item => item.fileName)
      .filter(name => !name.startsWith('.'));
    assert.ok(imports.length > 0, 'real source external imports discovered');
    for (const specifier of imports) {
      const name = specifier.split('/').slice(0, specifier.startsWith('@') ? 2 : 1).join('/');
      assert.equal(typeof venue.dependencies[name], 'string', `maintained source dependency ${specifier}`);
      assert.equal(actual.dependencies[name], venue.dependencies[name], `isolated source dependency ${specifier}`);
    }
  });
}
