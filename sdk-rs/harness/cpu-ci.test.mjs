import assert from 'node:assert/strict';
import { test } from 'node:test';
import { validatePlannedCases, requireSecondState } from './cpu-ci.mjs';
import { expectedGateCases } from './ci-gate-suites.mjs';

const dispositions = (decision) => [{ key: 'a'.repeat(64), decision,
  case: { ...expectedGateCases('bitget')[0], tapeSha256: 'b'.repeat(64), private: false, runs: 9, correctnessRuns: 1 } }];
const plan = () => ({ schema: 1, runId: 1, checkoutSha: 'c'.repeat(40), suites: [
  { suite: 'bitget', firstArtifactId: 2, firstArtifactDigest: 'sha256:' + 'd'.repeat(64), cases: dispositions('second') },
] });
const job = (extra = {}) => ({ id: 3, run_attempt: 1, status: 'completed', conclusion: 'success', ...extra });

test('a first PASS needs no second and cannot silently gain another timing sample', () => {
  assert.equal(requireSecondState(dispositions('first'), []), false);
  assert.equal(requireSecondState(dispositions('first'), [job({ conclusion: 'skipped' })]), false);
  for (const value of [[job()], [job({ status: 'queued', conclusion: null })], [job({ conclusion: 'failure' })]])
    assert.throws(() => requireSecondState(dispositions('first'), value), /without a first miss/);
});

test('eligible first miss requires exactly one successful original second; every incomplete/error/retry fails', () => {
  assert.equal(requireSecondState(dispositions('second'), [job()]), true);
  for (const value of [undefined, [], [job({ conclusion: 'failure' })], [job({ conclusion: 'cancelled' })],
    [job({ status: 'in_progress', conclusion: null })], [job({ run_attempt: 2 })], [job(), job({ id: 4 })]])
    assert.throws(() => requireSecondState(dispositions('second'), value));
});

test('the plan binds a complete Bitget public roster and never enables another suite or reuses an old pair', () => {
  assert.equal(validatePlannedCases(plan()).suites.length, 1);
  const changes = [p => { p.suites = []; }, p => { p.suites.push(structuredClone(p.suites[0])); },
    p => { p.suites[0].suite = 'bitgetusdc'; }, p => { p.suites[0].cases = []; },
    p => { p.suites[0].cases.push(structuredClone(p.suites[0].cases[0])); },
    p => { p.suites[0].cases[0].case.books = 'different'; }, p => { p.suites[0].cases[0].decision = 'reuse'; },
    p => { p.suites[0].cases[0].case.private = true; }, p => { p.suites[0].cases[0].case.runs = 7; },
    p => { p.checkoutSha = ''; }, p => { p.suites[0].firstArtifactDigest = ''; }];
  for (const change of changes) { const value = plan(); change(value); assert.throws(() => validatePlannedCases(value)); }
});
