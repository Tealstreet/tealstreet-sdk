import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { birthVerifier } from './accepted-births.mjs';

export function shrinks(previous, current, context, allowNew = () => false) {
  const prior = new Map(previous.entries.map(entry => [entry.type, entry]));
  for (const entry of current.entries) {
    if (!prior.has(entry.type)) {
      assert.ok(allowNew(entry), `${entry.type}: outstanding defect was added or re-added: ${context}`);
      continue;
    }
    const allowed = new Set(prior.get(entry.type).failures);
    for (const failure of entry.failures) assert.ok(allowed.has(failure), `${entry.type}: outstanding failure was added or re-added: ${failure}`);
  }
}
export function checkRatchet(results, debt) {
  assert.equal(debt.schema, 1, 'outstanding defects: unknown schema');
  const failures = new Map();
  for (const result of results) {
    for (const failure of result.failures) {
      assert.ok(/^(?:Deserialize|qualified decoder|entrypoint .+) accepted /.test(failure), `${result.name}: classification/positive-fixture failures cannot be outstanding shape debt: ${failure}`);
      const known = failures.get(result.name) ?? new Set();
      const configuration = result.configuration ?? { target: 'native', features: [] };
      known.add(`[${configuration.target};${[...configuration.features].sort().join(',')}] ${failure}`);
      failures.set(result.name, known);
    }
  }
  const listed = new Set();
  for (const entry of debt.entries) {
    assert.ok(!listed.has(entry.type), `${entry.type}: duplicate outstanding defect`);
    listed.add(entry.type);
    assert.ok(entry.owner?.crate && entry.owner?.agent, `${entry.type}: outstanding defect requires crate and agent owner`);
    assert.equal(entry.status, 'outstanding', `${entry.type}: repaired/reported-repaired defect must be removed`);
    const actual = failures.get(entry.type);
    assert.ok(actual?.size, `${entry.type}: repaired outstanding defect is still listed; remove it in the repair commit`);
    assert.deepEqual([...actual].sort(), [...entry.failures].sort(), `${entry.type}: outstanding accepted shapes changed; new widening is forbidden and repaired shapes must be removed`);
  }
  for (const name of failures.keys()) assert.ok(listed.has(name), `${name}: NEW deserialization widening; classify and prove declaration birth or repair it`);
  return failures.size;
}
// Mirror seed that restored this ledger byte-identical to monorepo 266f305ac1's
// reviewed lineage; the monorepo history is not an ancestor in this repository.
const origin = { commit: '91a50c7a2c14cc5e291c4b4f0633fde32f752757', path: 'sdk-rs/safe-cex-parity/accepted-set/outstanding-defects.json' };
export function checkHistory(repo, relative, debt, options = {}) {
  function git(args, optional = false) {
    const result = spawnSync('git', args, { cwd: repo, encoding: 'utf8', maxBuffer: 128 * 1024 * 1024 });
    if (optional && result.status !== 0) return undefined;
    assert.equal(result.status, 0, `outstanding defects history: ${result.stderr}`);
    return result.stdout.trim();
  }
  const anchor = options.origin ?? origin;
  assert.equal(relative, anchor.path, 'outstanding defects: ledger path is anchored; rename requires an explicit reviewed migration');
  assert.equal(git(['rev-parse', '--is-shallow-repository']), 'false', 'outstanding defects history requires full checkout history');
  const ancestor = (before, after) => git(['merge-base', '--is-ancestor', before, after], true) !== undefined;
  assert.ok(ancestor(anchor.commit, 'HEAD'), 'outstanding defects: reviewed origin must be in ancestry');
  const baseline = JSON.parse(git(['show', `${anchor.commit}:${relative}`]));
  const born = new Map(baseline.entries.map(entry => [entry.type, anchor.commit]));
  const verifyBirth = birthVerifier(repo, git, options.declarations);
  const read = commit => {
    const value = git(['show', `${commit}:${relative}`], true);
    assert.ok(value || !ancestor(anchor.commit, commit), `outstanding defects file was deleted at ${commit}; retain an empty list`);
    return value ? JSON.parse(value) : { entries: [] };
  };
  function transition(commit, parents, current) {
    const previous = parents.map(read);
    for (const entry of current.entries) {
      if (!born.has(entry.type)) {
        assert.ok(!previous.some(value => value.entries.some(prior => prior.type === entry.type)), `${entry.type}: missing debt birth provenance`);
        verifyBirth(entry, commit, parents);
        born.set(entry.type, commit);
      }
    }
    parents.forEach((parent, i) => shrinks(previous[i], current, commit, entry => {
      const birth = born.get(entry.type);
      if (birth === commit) return true;
      // A branch predating introduction is neutral; a repair after introduction is binding.
      if (ancestor(birth, parent)) return false;
      if (birth !== anchor.commit) verifyBirth(entry, commit, [parent]);
      return true;
    }));
  }
  const changes = git(['log', '--full-history', '--topo-order', '--reverse', '--format=%H %P', `${anchor.commit}..HEAD`, '--', relative]);
  for (const line of changes.split('\n').filter(Boolean)) {
    const [commit, ...parents] = line.split(' ');
    transition(commit, parents, read(commit));
  }
  const baselineOwners = new Set(baseline.entries.map(entry => entry.type));
  for (const entry of debt.entries) if (!baselineOwners.has(entry.type)) {
    assert.equal(options.declarations?.get(entry.type), entry.declaration, `${entry.type}: debt declaration must match the current production compiler owner`);
  }
  transition('WORKTREE', [git(['rev-parse', 'HEAD'])], debt);
}
