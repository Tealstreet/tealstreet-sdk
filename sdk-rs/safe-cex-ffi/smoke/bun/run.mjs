import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { packageVersion } from '../../scripts/tooling.mjs';

assert.ok(process.versions.bun, 'this smoke must execute in Bun');
const require = createRequire(import.meta.url);
const { parity_probe, version } = require('../../dist/node/safe_cex_ffi.node');
const expectedVersion = packageVersion();
for (const n of [0, 42, 4294967295]) assert.equal(parity_probe(n), `safe-cex:${n}`);
assert.equal(version(), expectedVersion);
console.log(`Bun native smoke passed: parity_probe(42) = ${parity_probe(42)}, version = ${version()}`);
