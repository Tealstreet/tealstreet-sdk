import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

export function checkExtractorDependency(root) {
  const extractor = path.join(root, 'sdk-rs/parity/extract-ts');
  const rootPackage = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
  const extractorPackage = JSON.parse(fs.readFileSync(path.join(extractor, 'package.json'), 'utf8'));
  assert.ok(rootPackage.workspaces.includes('sdk-rs/parity/extract-ts'), 'parity extractor dependency: package must be a root Yarn workspace');
  const version = extractorPackage.dependencies?.['smol-toml'];
  assert.ok(typeof version === 'string' && version, 'parity extractor dependency: smol-toml must be declared by the extractor');
  const lock = fs.readFileSync(path.join(root, 'yarn.lock'), 'utf8');
  assert.match(lock, new RegExp(`^smol-toml@${version.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}:$`, 'm'), 'parity extractor dependency: smol-toml version must be present in the root Yarn lockfile');
  try {
    createRequire(path.join(extractor, 'package.json')).resolve('smol-toml');
  } catch (error) {
    assert.fail(`parity extractor dependency: smol-toml is unavailable after install (${error.code ?? error.message})`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    // scripts/ -> extract-ts -> parity -> sdk-rs -> repo root. The check reads
    // the ROOT manifest and lockfile, so it runs unchanged in the monorepo and
    // in the mirror, where this slice is the whole tree.
    checkExtractorDependency(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..'));
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
