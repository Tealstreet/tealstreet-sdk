import assert from 'node:assert/strict';
import test from 'node:test';
import { assessCiResult, landingOwnership } from './ci-result.mjs';
const selection = { full: false, nativeFull: false, combinedRequired: false, replayRequired: true, sizeRequired: true };
const jobs = Object.fromEntries(['selection', 'bans', 'harness', 'native', 'replay', 'families'].map(name => [name, { result: 'success' }]));

test('PR acceptance depends on selected correctness coverage, never CPU certification', () => {
  const result = assessCiResult({ pullRequest: true, selection, jobs: { ...jobs, 'cpu-certify': { result: 'failure' } } });
  assert.equal(result.accepted, true);
  for (const name of result.required)
    assert.equal(assessCiResult({ pullRequest: true, selection, jobs: { ...jobs, [name]: { result: 'skipped' } } }).accepted, false, name);
});

test('a red landing identifies its PR owner and retains actor ownership on lookup failure', async () => {
  const sha = 'a'.repeat(40);
  assert.match(await landingOwnership({ sha, actor: 'merger', reader: {
    listCommitPulls: async () => [{ merge_commit_sha: sha, user: { login: 'author' }, number: 12, head: { ref: 'fix/example' } }],
  } }), /@author, PR #12/);
  assert.match(await landingOwnership({ sha, actor: 'merger', reader: {
    listCommitPulls: async () => { throw new Error('HTTP 503'); },
  } }), /@merger, landing/);
});

test('docs/tests can omit replay and size; shared and landing checks cannot', () => {
  const bare = { ...selection, replayRequired: false, sizeRequired: false };
  const noReplay = { ...jobs, replay: { result: 'skipped' }, families: { result: 'skipped' } };
  assert.equal(assessCiResult({ pullRequest: true, selection: bare, jobs: noReplay }).accepted, true);
  assert.throws(() => assessCiResult({ pullRequest: true, selection: { ...bare, full: true, nativeFull: true }, jobs: noReplay }));
  assert.equal(assessCiResult({ pullRequest: true, selection: { ...bare, combinedRequired: true }, jobs: noReplay }).accepted, false);
  const full = { ...selection, full: true, nativeFull: true, combinedRequired: true };
  const complete = { ...jobs, ...Object.fromEntries(['wasm', 'combined', 'browser', 'bitget-paired', 'cpu-plan', 'cpu-certify', 'cpu-final']
    .map(name => [name, { result: 'success' }])) };
  assert.equal(assessCiResult({ pullRequest: false, selection: full, jobs: complete }).accepted, true);
  for (const name of ['wasm', 'combined'])
    assert.equal(assessCiResult({ pullRequest: false, selection: full, jobs: { ...complete, [name]: { result: 'failure' } } }).accepted, false, name);
  assert.throws(() => assessCiResult({ pullRequest: true, selection: {}, jobs }));
});

test('CPU and certification remain visible but never block the correctness aggregate on either event', () => {
  for (const pullRequest of [true, false]) {
    const full = { full: true, nativeFull: true, combinedRequired: true, replayRequired: true, sizeRequired: true };
    const complete = { ...jobs, wasm: { result: 'success' }, combined: { result: 'success' } };
    for (const name of ['gate', 'bitget-paired', 'cpu-plan', 'cpu-second', 'cpu-certify', 'cpu-final']) {
      const result = assessCiResult({ pullRequest, selection: full, jobs: { ...complete, [name]: { result: 'failure' } } });
      assert.equal(result.accepted, true, name);
      assert(!result.required.includes(name));
    }
  }
});
