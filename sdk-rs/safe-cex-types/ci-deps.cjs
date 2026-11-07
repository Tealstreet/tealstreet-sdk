const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '../..');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
const target = process.argv[2];
assert(target && path.isAbsolute(target), 'provide an absolute isolated dependency directory');
fs.mkdirSync(target, { recursive: true });
fs.writeFileSync(path.join(target, 'package.json'), JSON.stringify({
  private: true,
  packageManager: manifest.packageManager,
  dependencies: {
    vite: JSON.parse(fs.readFileSync(path.join(root, 'packages/safe-cex/package.json'), 'utf8')).devDependencies.vite,
    vitest: JSON.parse(fs.readFileSync(path.join(root, 'packages/safe-cex/package.json'), 'utf8')).devDependencies.vitest,
    typescript: manifest.devDependencies.typescript,
    // Venue signing and exchange oracles execute real hashing, numeric and Axios boundaries.
    ...Object.fromEntries(['axios', 'qs', 'create-hmac', 'axios-retry', 'axios-rate-limit', 'nanoid', 'tiny-emitter', 'bignumber.js', '@noble/hashes', '@ethersproject/bignumber', '@noble/ed25519', 'bs58'].map(name => [name,
      JSON.parse(fs.readFileSync(path.join(root, 'packages/safe-cex/package.json'), 'utf8')).dependencies[name],
    ])),
    // Store clock oracle uses real dayjs; keep its package selector and lockfile.
    lodash: JSON.parse(fs.readFileSync(path.join(root, 'packages/safe-cex/package.json'), 'utf8')).dependencies.lodash,
    'p-iteration': JSON.parse(fs.readFileSync(path.join(root, 'packages/safe-cex/package.json'), 'utf8')).dependencies['p-iteration'],
    dayjs: JSON.parse(fs.readFileSync(path.join(root, 'packages/safe-cex/package.json'), 'utf8')).dependencies.dayjs,
    // Aster builder/signing live oracle executes the real ethers package.
    ethers: JSON.parse(fs.readFileSync(path.join(root, 'packages/safe-cex/package.json'), 'utf8')).dependencies.ethers,
  },
}, null, 2) + '\n');
fs.copyFileSync(path.join(root, 'yarn.lock'), path.join(target, 'yarn.lock'));
