import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseJsonWithSource } from './accepted-json-source.mjs';

export const defaultContractDirectory = new URL('../accepted-set/contracts/', import.meta.url);

export { stringifyJsonWithSource as contractJson } from './accepted-json-source.mjs';

function readObject(file) {
  const value = parseJsonWithSource(fs.readFileSync(file, 'utf8'));
  assert.ok(value && typeof value === 'object' && !Array.isArray(value), `accepted-set: contracts must be an object: ${file}`);
  return value;
}

// Explicit JSON files remain supported for isolated --contracts fixtures. The
// production directory has one file per compiler crate; no row is reclassified.
export function loadContracts(source = defaultContractDirectory) {
  if (source instanceof URL) source = fileURLToPath(source);
  const info = fs.statSync(source);
  if (info.isFile()) return readObject(source);
  assert.ok(info.isDirectory(), 'accepted-set: contracts source must be a regular file or directory');
  const files = fs.readdirSync(source).sort();
  assert.ok(files.length, 'accepted-set: contracts directory must not be empty');
  const entries = [];
  const seen = new Set();
  for (const name of files) {
    assert.match(name, /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*\.json$/, 'accepted-set: unexpected contract filename');
    const file = path.join(source, name);
    assert.ok(fs.lstatSync(file).isFile(), `accepted-set: contract shard must be a regular file: ${name}`);
    const prefix = name.slice(0, -5).replaceAll('-', '_') + '::';
    const rows = Object.entries(readObject(file));
    assert.ok(rows.length, `accepted-set: empty contract shard: ${name}`);
    for (const [key, value] of rows) {
      assert.ok(!seen.has(key), `accepted-set: duplicate contract owner: ${key}`);
      assert.ok(key.startsWith(prefix) && key.length > prefix.length, `accepted-set: contract owner belongs in another crate file: ${key} in ${name}`);
      seen.add(key);
      entries.push([key, value]);
    }
  }
  return Object.fromEntries(entries);
}
