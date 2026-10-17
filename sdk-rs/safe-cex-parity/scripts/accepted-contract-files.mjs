import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const defaultContractDirectory = new URL('../accepted-set/contracts/', import.meta.url);

// JSON.parse orders integer keys before Rust sees them. Preserve the authored
// order separately so generated dispatch wrappers can retain their input children.
const authoredKeys = new WeakMap();

function rememberKeyOrder(text, value) {
  const tokens = /\s*("(?:\\[\s\S]|[^"\\])*"|[{}\[\],:]|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?|true|false|null)/y;
  const next = () => {
    const token = tokens.exec(text);
    assert.ok(token, 'accepted-set: expected validated JSON token');
    return token[1];
  };
  function visit(value, token = next()) {
    if (token === '{') {
      const keys = [];
      let key = next();
      while (key !== '}') {
        const name = JSON.parse(key);
        assert.ok(!keys.includes(name), `accepted-set: duplicate JSON key: ${name}`);
        keys.push(name);
        assert.equal(next(), ':');
        visit(value[name]);
        const separator = next();
        if (separator === '}') break;
        assert.equal(separator, ',');
        key = next();
      }
      authoredKeys.set(value, keys);
    } else if (token === '[') {
      let item = next();
      let index = 0;
      while (item !== ']') {
        visit(value[index++], item);
        const separator = next();
        if (separator === ']') break;
        assert.equal(separator, ',');
        item = next();
      }
      assert.equal(index, value.length);
    }
  }
  visit(value);
  assert.match(text.slice(tokens.lastIndex), /^\s*$/);
}

export function contractJson(value) {
  if (Array.isArray(value)) return `[${value.map(item => contractJson(item) ?? 'null').join(',')}]`;
  if (value && typeof value === 'object') {
    const original = authoredKeys.get(value) ?? [];
    const keys = [...original.filter(key => Object.hasOwn(value, key)), ...Object.keys(value).filter(key => !original.includes(key))];
    const fields = keys.flatMap(key => {
      const encoded = contractJson(value[key]);
      return encoded === undefined ? [] : [`${JSON.stringify(key)}:${encoded}`];
    });
    return `{${fields.join(',')}}`;
  }
  return Object.is(value, -0) ? '-0' : JSON.stringify(value);
}

function readObject(file) {
  const text = fs.readFileSync(file, 'utf8');
  const value = JSON.parse(text);
  assert.ok(value && typeof value === 'object' && !Array.isArray(value), `accepted-set: contracts must be an object: ${file}`);
  rememberKeyOrder(text, value);
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
