const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
const sdk = JSON.parse(fs.readFileSync(path.join(root, 'packages/safe-cex/package.json'), 'utf8'));
const target = process.argv[2];
assert(target && path.isAbsolute(target), 'provide an absolute isolated core dependency directory');
fs.mkdirSync(target, { recursive: true });
fs.writeFileSync(path.join(target, 'package.json'), JSON.stringify({
  private: true,
  packageManager: manifest.packageManager,
  dependencies: {
    vite: JSON.parse(fs.readFileSync(path.join(root, 'packages/safe-cex/package.json'), 'utf8')).devDependencies.vite,
    vitest: JSON.parse(fs.readFileSync(path.join(root, 'packages/safe-cex/package.json'), 'utf8')).devDependencies.vitest, "tiny-emitter": sdk.dependencies["tiny-emitter"], nanoid: sdk.dependencies.nanoid, axios: sdk.dependencies.axios, typescript: manifest.devDependencies.typescript, jsonwebtoken: sdk.dependencies.jsonwebtoken, "axios-retry": sdk.dependencies["axios-retry"], lodash: sdk.dependencies.lodash, qs: sdk.dependencies.qs },
}, null, 2) + '\n');
// Repository selectors and frozen lockfile pin the actual dependency graph.
fs.copyFileSync(path.join(root, 'yarn.lock'), path.join(target, 'yarn.lock'));
