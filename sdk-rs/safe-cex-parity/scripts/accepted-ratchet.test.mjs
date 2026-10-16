import assert from 'node:assert/strict';
import test from 'node:test';
import { checkRatchet, shrinks } from './accepted-ratchet.mjs';
const entry = { type: 'crate::Side', owner: { crate: 'crate', agent: 'owner' }, status: 'outstanding', failures: ['[native;] Deserialize accepted {"buy":null}'] };
const debt = { schema: 1, entries: [entry] };
const rows = [{ name: entry.type, failures: ['Deserialize accepted {"buy":null}'] }];
test('outstanding debt exactly matches measured and owned failures', () => {
  assert.equal(checkRatchet(rows, debt), 1);
  assert.throws(() => checkRatchet([...rows, { name: 'crate::New', failures: ['Deserialize accepted []'] }], debt), /crate::New: NEW/);
  assert.throws(() => checkRatchet(rows, { ...debt, entries: [{ ...entry, owner: {} }] }), /requires crate and agent/);
});
test('repair requires immediate removal, including partial shape repairs', () => {
  assert.throws(() => checkRatchet([{ name: entry.type, failures: [] }], debt), /repaired outstanding defect is still listed/);
  assert.throws(() => checkRatchet(rows, { ...debt, entries: [{ ...entry, status: 'repaired' }] }), /reported-repaired/);
});
test('a removed defect cannot return even when total count stays below its original size', () => {
  assert.throws(() => shrinks({ entries: [] }, debt, 'control'), /added or re-added/);
  assert.throws(() => shrinks({ entries: [{ ...entry, type: 'crate::Other' }] }, debt, 'control'), /added or re-added/);
  shrinks(debt, { entries: [] }, 'valid repair');
});
test('unclassified types and fixture failures cannot be ratcheted', () => {
  for (const failure of ['unclassified type', 'positive record sample is not accepted JSON']) assert.throws(() => checkRatchet([{ name: entry.type, failures: [failure] }], debt), /cannot be outstanding shape debt/);
});

test('real git ancestry rejects a removed defect being re-added', async () => {
  const fs = await import('node:fs');
  const path = await import('node:path');
  const { spawnSync } = await import('node:child_process');
  const { checkHistory } = await import('./accepted-ratchet.mjs');
  const parent = new URL('../../target/', import.meta.url);
  fs.mkdirSync(parent, { recursive: true });
  const directory = fs.mkdtempSync(path.join(parent.pathname, 'accepted-history-'));
  const git = (...args) => {
    const result = spawnSync('git', args, { cwd: directory, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
  };
  const commit = value => {
    fs.writeFileSync(path.join(directory, 'outstanding.json'), JSON.stringify(value));
    git('add', 'outstanding.json');
    git('-c', 'user.name=Gate control', '-c', 'user.email=gate@example.invalid', '-c', 'core.hooksPath=/dev/null', 'commit', '-qm', 'control');
  };
  try {
    git('init', '-q');
    commit(debt);
    const origin = { commit: spawnSync('git', ['rev-parse', 'HEAD'], { cwd: directory, encoding: 'utf8' }).stdout.trim(), path: 'outstanding.json' };
    commit({ schema: 1, entries: [] });
    checkHistory(directory, 'outstanding.json', { schema: 1, entries: [] }, { origin });
    commit(debt);
    assert.throws(() => checkHistory(directory, 'outstanding.json', debt, { origin }), /added or re-added/);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test('widening another configuration cannot hide behind existing native debt', () => {
  assert.throws(() => checkRatchet([...rows, { ...rows[0], configuration: { target: 'wasm32-unknown-unknown', features: [] } }], debt), /accepted shapes changed/);
});

test('merge restoring debt from a stale parent cannot hide repair ancestry', async () => {
  const fs = await import('node:fs');
  const path = await import('node:path');
  const { spawnSync } = await import('node:child_process');
  const { checkHistory } = await import('./accepted-ratchet.mjs');
  const parent = new URL('../../target/', import.meta.url);
  fs.mkdirSync(parent, { recursive: true });
  const directory = fs.mkdtempSync(path.join(parent.pathname, 'accepted-merge-history-'));
  const git = (...args) => {
    const result = spawnSync('git', ['-c', 'user.name=Gate control', '-c', 'user.email=gate@example.invalid', '-c', 'core.hooksPath=/dev/null', ...args], { cwd: directory, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  };
  try {
    git('init', '-q');
    fs.writeFileSync(path.join(directory, 'outstanding.json'), JSON.stringify(debt));
    git('add', '.'); git('commit', '-qm', 'original debt');
    const base = git('rev-parse', 'HEAD');
    const origin = { commit: base, path: 'outstanding.json' };
    const tree = git('rev-parse', 'HEAD^{tree}');
    fs.writeFileSync(path.join(directory, 'outstanding.json'), JSON.stringify({ schema: 1, entries: [] }));
    git('add', '.'); git('commit', '-qm', 'repair');
    const repaired = git('rev-parse', 'HEAD');
    const stale = git('commit-tree', tree, '-p', base, '-m', 'stale sibling');
    const merge = git('commit-tree', tree, '-p', repaired, '-p', stale, '-m', 'restore stale debt');
    git('update-ref', 'HEAD', merge);
    assert.throws(() => checkHistory(directory, 'outstanding.json', debt, { origin }), /added or re-added/);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});


test('new debt requires same-commit nominal birth and cannot follow a rename', async () => {
  const fs = await import('node:fs');
  const path = await import('node:path');
  const { spawnSync } = await import('node:child_process');
  const { checkHistory } = await import('./accepted-ratchet.mjs');
  const parent = new URL('../../target/', import.meta.url);
  fs.mkdirSync(parent, { recursive: true });
  const directory = fs.mkdtempSync(path.join(parent.pathname, 'accepted-birth-history-'));
  const git = (...args) => {
    const result = spawnSync('git', ['-c', 'user.name=Gate control', '-c', 'user.email=gate@example.invalid', '-c', 'core.hooksPath=/dev/null', ...args], { cwd: directory, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr); return result.stdout.trim();
  };
  const source = 'sdk-rs/crate/src/lib.rs';
  const write = (file, value) => { fs.mkdirSync(path.dirname(path.join(directory, file)), { recursive: true }); fs.writeFileSync(path.join(directory, file), value); };
  const commit = () => { git('add', '.'); git('commit', '-qm', 'control'); return git('rev-parse', 'HEAD'); };
  const empty = { schema: 1, entries: [] };
  const next = { schema: 1, entries: [{ ...entry, declaration: source }] };
  try {
    git('init', '-q');
    write('outstanding.json', JSON.stringify(empty));
    write(source, 'pub struct Existing;');
    const origin = { commit: commit(), path: 'outstanding.json' };
    const options = { origin, declarations: new Map([[entry.type, source]]) };
    write(source, 'pub struct Existing; pub enum Side { Buy }');
    write('outstanding.json', JSON.stringify(next));
    checkHistory(directory, 'outstanding.json', next, options);
    commit();
    checkHistory(directory, 'outstanding.json', next, options);
    const established = git('rev-parse', 'HEAD');
    write('outstanding.json', JSON.stringify(empty)); commit();
    write('outstanding.json', JSON.stringify(next));
    assert.throws(() => checkHistory(directory, 'outstanding.json', next, options), /added or re-added/);
    git('reset', '--hard', established);
    const old = { schema: 1, entries: [{ ...entry, type: 'crate::Existing', declaration: source }] };
    options.declarations.set('crate::Existing', source);
    assert.throws(() => checkHistory(directory, 'outstanding.json', old, options), /pre-existing declaration/);
    git('mv', 'outstanding.json', 'moved.json');
    write('moved.json', JSON.stringify({ ...next, entries: [...next.entries, ...old.entries] })); commit();
    assert.throws(() => checkHistory(directory, 'moved.json', next, options), /ledger path is anchored/);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});

test('opaque history, dormant declarations, and recreated names cannot claim nominal birth', async () => {
  const fs = await import('node:fs'); const path = await import('node:path');
  const { spawnSync } = await import('node:child_process'); const { checkHistory } = await import('./accepted-ratchet.mjs');
  const parent = new URL('../../target/', import.meta.url);
  const cases = [
    ['#[cfg(any())] pub enum Side { Buy }', /pre-existing declaration/],
    ['pub enum Side { Buy }', /pre-existing declaration/],
    ['macro_rules! make { () => { pub struct Side; } } make!();', /historical owner expansion/],
    ['#[path = "../../../infra/old.rs"] mod wire;', /historical owner expansion/],
    ['#[cfg_attr(feature = "old", path = "../../../infra/old.rs")] mod wire;', /historical owner expansion/],
  ];
  for (const [oldSource, expected] of cases) {
    const directory = fs.mkdtempSync(path.join(parent.pathname, 'accepted-opaque-birth-'));
    const git = (...args) => { const r = spawnSync('git', ['-c', 'user.name=Control', '-c', 'user.email=control@example.invalid', '-c', 'core.hooksPath=/dev/null', ...args], { cwd: directory, encoding: 'utf8' }); assert.equal(r.status, 0, r.stderr); return r.stdout.trim(); };
    const source = 'sdk-rs/crate/src/lib.rs';
    try {
      git('init', '-q'); fs.mkdirSync(path.join(directory, 'sdk-rs/crate/src'), { recursive: true });
      fs.writeFileSync(path.join(directory, source), oldSource);
      fs.writeFileSync(path.join(directory, 'sdk-rs/crate/src/aaa.txt'), oldSource);
      fs.writeFileSync(path.join(directory, 'outstanding.json'), JSON.stringify({ schema: 1, entries: [] }));
      git('add', '.'); git('commit', '-qm', 'historical declaration');
      const origin = { commit: git('rev-parse', 'HEAD'), path: 'outstanding.json' };
      fs.writeFileSync(path.join(directory, source), 'pub enum Side { Buy }');
      const current = { schema: 1, entries: [{ ...entry, declaration: source }] };
      const options = { origin, declarations: new Map([[entry.type, source]]) };
      assert.throws(() => checkHistory(directory, origin.path, current, options), expected);
      if (oldSource.startsWith('#[cfg(any())]')) {
        fs.writeFileSync(path.join(directory, source), ''); git('add', '.'); git('commit', '-qm', 'delete old declaration');
        fs.writeFileSync(path.join(directory, source), 'pub enum Side { Buy }');
        assert.throws(() => checkHistory(directory, origin.path, current, options), /pre-existing declaration/);
      }
    } finally { fs.rmSync(directory, { recursive: true, force: true }); }
  }
});

test('sibling introductions cannot launder delayed debt but genuine new-owner merges pass', async () => {
  const fs = await import('node:fs'); const path = await import('node:path');
  const { spawnSync } = await import('node:child_process'); const { checkHistory } = await import('./accepted-ratchet.mjs');
  const parent = new URL('../../target/', import.meta.url);
  for (const delayed of [false, true]) {
    const directory = fs.mkdtempSync(path.join(parent.pathname, 'accepted-sibling-birth-'));
    const git = (...args) => { const r = spawnSync('git', ['-c', 'user.name=Control', '-c', 'user.email=control@example.invalid', '-c', 'core.hooksPath=/dev/null', ...args], { cwd: directory, encoding: 'utf8' }); assert.equal(r.status, 0, r.stderr); return r.stdout.trim(); };
    const source = 'sdk-rs/crate/src/lib.rs';
    const write = (file, text) => fs.writeFileSync(path.join(directory, file), text);
    const commit = () => { git('add', '.'); git('commit', '-qm', 'control'); return git('rev-parse', 'HEAD'); };
    try {
      git('init', '-q'); fs.mkdirSync(path.join(directory, 'sdk-rs/crate/src'), { recursive: true });
      write(source, 'pub struct Base;'); write('outstanding.json', JSON.stringify({ schema: 1, entries: [] }));
      const origin = { commit: commit(), path: 'outstanding.json' };
      const current = { schema: 1, entries: [{ ...entry, declaration: source }] };
      write(source, 'pub struct Base; pub enum Side { Buy }'); write('outstanding.json', JSON.stringify(current));
      const left = commit(); const tree = git('rev-parse', 'HEAD^{tree}');
      git('checkout', '--detach', origin.commit);
      write(source, delayed ? 'pub struct Base; pub enum Side { Buy }' : 'pub struct Base; pub struct Other;');
      let right = commit();
      if (delayed) { write('outstanding.json', JSON.stringify(current)); right = commit(); }
      const merge = git('commit-tree', tree, '-p', left, '-p', right, '-m', 'merge');
      git('reset', '--hard', merge);
      const options = { origin, declarations: new Map([[entry.type, source]]) };
      if (delayed) assert.throws(() => checkHistory(directory, origin.path, current, options), /pre-existing declaration/);
      else checkHistory(directory, origin.path, current, options);
    } finally { fs.rmSync(directory, { recursive: true, force: true }); }
  }
});
