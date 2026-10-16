import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { buildScanner } from './accepted-scanner.mjs';

// This parser establishes declaration birth, never decoder behavior.
export function birthVerifier(repo, git, declarations = new Map()) {
  const names = new Map();
  const histories = new Map();
  let scanner;
  function parse(sources) {
    scanner ??= buildScanner();
    const result = spawnSync(scanner, ['--nominal-json'], { input: JSON.stringify(sources), encoding: 'utf8', maxBuffer: 128 * 1024 * 1024 });
    assert.equal(result.status, 0, `outstanding defects: declaration parser failed: ${result.error ?? result.stderr}`);
    const parsed = JSON.parse(result.stdout);
    assert.equal(parsed.length, sources.length, 'outstanding defects: incomplete declaration inventory');
    return parsed;
  }
  function historicalNames(parent, prefix, external) {
    const key = `${parent}:${prefix}:${external}`;
    if (histories.has(key)) return histories.get(key);
    const changes = git(['log', '--full-history', '--root', '-m', '--raw', '--format=', '--no-abbrev', '--no-renames', '-z', parent, '--', prefix, external]).split('\0');
    const objects = [];
    for (let i = 0; i < changes.length; i++) {
      const header = changes[i].trim();
      if (!header.startsWith(':')) continue;
      const file = changes[++i];
      if (!file?.endsWith('.rs')) continue;
      const [, , before, after] = header.split(' ');
      for (const id of [before, after]) if (id && !/^0+$/.test(id)) objects.push(id);
    }
    const missing = [...new Set(objects)].filter(id => !names.has(id));
    if (missing.length) {
      const result = spawnSync('git', ['cat-file', '--batch'], { cwd: repo, input: `${missing.join('\n')}\n`, maxBuffer: 128 * 1024 * 1024 });
      assert.equal(result.status, 0, 'outstanding defects: historical source objects unavailable');
      let at = 0;
      const sources = [];
      for (const id of missing) {
        const end = result.stdout.indexOf(10, at);
        const [actual, kind, length] = result.stdout.subarray(at, end).toString().split(' ');
        assert.equal(actual, id); assert.equal(kind, 'blob');
        at = end + 1;
        sources.push(result.stdout.subarray(at, at + Number(length)).toString());
        at += Number(length) + 1;
      }
      const parsed = parse(sources);
      missing.forEach((id, i) => names.set(id, parsed[i]));
    }
    const result = new Set(objects.flatMap(id => names.get(id)));
    histories.set(key, result);
    return result;
  }
  return (entry, commit, parents) => {
    const file = entry.declaration;
    const ident = entry.type.split('::').at(-1).replace(/^r#/, '');
    const crate = entry.owner.crate;
    assert.match(crate, /^[a-z][a-z0-9-]+$/, `${entry.type}: invalid owner crate`);
    assert.equal(entry.type.split('::')[0], crate.replaceAll('-', '_'), `${entry.type}: declaration crate mismatch`);
    assert.ok(typeof file === 'string' && file.endsWith('.rs') && !path.isAbsolute(file) && !file.split('/').includes('..'), `${entry.type}: new debt requires a repository-relative declaration source`);
    assert.ok(file.startsWith(`sdk-rs/${crate}/src/`), `${entry.type}: external source owners need explicit birth provenance; new debt is not permitted`);
    if (commit === 'WORKTREE') assert.equal(declarations.get(entry.type), file, `${entry.type}: new debt declaration must match the production compiler owner`);
    const source = commit === 'WORKTREE' ? fs.readFileSync(path.join(repo, file), 'utf8') : git(['show', `${commit}:${file}`]);
    assert.ok(parse([source])[0].includes(ident), `${entry.type}: new debt requires its nominal declaration in the same commit`);
    for (const parent of parents) {
      // Same-name relocation/recreation is deliberately conservative: it is not a new type.
      assert.ok(!historicalNames(parent, `sdk-rs/${crate}/src`, file).has(ident), `${entry.type}: debt added to a pre-existing declaration; only a new owner in the same commit may add debt`);
    }
  };
}
