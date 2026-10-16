import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const defaultPolicyRoot = new URL('../accepted-set/', import.meta.url);

// Compiler owners and source-scope helpers use different existing key formats.
// Ownership comes from the key itself; a shard cannot redirect another crate.
export function policyCrate(key) {
  assert.equal(typeof key, 'string', 'accepted-set: policy owner must be a string');
  const compiler = key.match(/^([a-z][a-z0-9]*(?:_[a-z0-9]+)*)::.+$/);
  if (compiler) return compiler[1].replaceAll('_', '-');
  const source = key.match(/^([a-z][a-z0-9]*(?:-[a-z0-9]+)*)\/(?:[A-Za-z0-9_-]+\/)*[A-Za-z0-9_-]+\.rs::.+$/);
  assert.ok(source, `accepted-set: unsupported policy owner: ${key}`);
  return source[1];
}

// Direct cutover: production policy maps live only in <table>/<crate>.json.
// Existing downstream signature, trait, source-pin and native checks still
// receive the same merged object. Fixtures may supply an isolated policy root.
export function loadPolicy(name, root = defaultPolicyRoot) {
  assert.match(name, /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/, 'accepted-set: invalid policy table name');
  if (root instanceof URL) root = fileURLToPath(root);
  assert.ok(!fs.existsSync(path.join(root, name + '.json')), `accepted-set: obsolete flat policy must stay deleted: ${name}.json`);
  const directory = path.join(root, name);
  assert.ok(fs.lstatSync(directory).isDirectory(), `accepted-set: policy table must be a directory: ${name}`);
  const files = fs.readdirSync(directory).sort();
  assert.ok(files.length, `accepted-set: policy table must not be empty: ${name}`);
  const entries = [], seen = new Set();
  for (const file of files) {
    assert.match(file, /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*\.json$/, 'accepted-set: unexpected policy filename');
    const source = path.join(directory, file);
    assert.ok(fs.lstatSync(source).isFile(), `accepted-set: policy shard must be a regular file: ${file}`);
    const value = JSON.parse(fs.readFileSync(source, 'utf8'));
    assert.ok(value && typeof value === 'object' && !Array.isArray(value), `accepted-set: policy shard must be an object: ${file}`);
    const rows = Object.entries(value);
    assert.ok(rows.length, `accepted-set: empty policy shard: ${file}`);
    for (const [key, record] of rows) {
      assert.ok(!seen.has(key), `accepted-set: duplicate policy owner: ${key}`);
      assert.equal(policyCrate(key), file.slice(0, -5), `accepted-set: policy owner belongs in another crate file: ${key} in ${file}`);
      seen.add(key);
      entries.push([key, record]);
    }
  }
  return Object.fromEntries(entries);
}
