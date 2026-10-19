// One approved full WOO control/candidate pair, two native repeats per side.
// Compare every retained output without normalization; SHA checks file integrity.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { deserialize } from 'node:v8';
import { assertOutputIdentity, runIdentityGuardControls } from '../../scripts/live-diff/observable-output-identity.mjs';

const paths = process.argv.slice(2);
assert.equal(paths.length, 2, 'Supply the one approved full WOO control/candidate pair');
const reports = paths.map(path => JSON.parse(readFileSync(path)));
const bySide = reports.map(report => report.runs.filter(run => run.implementation === 'rust'));
assert.notEqual(reports[0].addonSha256, reports[1].addonSha256);
assert(Date.parse(reports[0].at) <= Date.parse(reports[1].at));
const controls = runIdentityGuardControls();
const graphs = [];
for (const [side, report] of reports.entries()) {
  assert.equal(report.nativeDrainVerified, true);
  assert.equal(report.comparisonsProvisional, false);
  assert.equal(report.capture.frames, 260);
  assert.equal(report.node, reports[0].node);
  assert.equal(report.capture.reusedBundle, reports[0].capture.reusedBundle);
  assert.equal(createHash('sha256').update(readFileSync(report.capture.reusedBundle)).digest('hex'), '03eef68dc0fa2994b6f24a0fa64b3b1474c75fb868877397e41e3bd1c6c4b316');
  for (const topic of ['tickers', 'bbos', 'markprices']) assert.equal(report.capture.topics[topic], 60);
  assert.equal(bySide[side].length, 2);
  assert.equal(report.runs.filter(run => run.implementation === 'ts').length, 2);
  for (const run of report.runs) {
    assert(run.requests.every(request => request.cacheHit === true), run.label + ' REST replay miss');
    const bytes = readFileSync(run.fixture);
    assert.equal(createHash('sha256').update(bytes).digest('hex'), run.outputHash);
    const value = deserialize(bytes);
    const implementation = run.implementation;
    const first = graphs.find(item => item.implementation === implementation);
    const observation = first ? assertOutputIdentity(first.value, value) : null;
    graphs.push({ implementation, side, label: run.label, fixture: run.fixture, fixtureSha256: run.outputHash, value, observation });
    assert.equal(run.receipt.delivered, 260);
    assert.equal(run.receipt.dropped, 0);
    assert.equal(run.receipt.inputHash, reports[0].runs[0].receipt.inputHash);
    assert.equal(run.receipt.inputHash, run.receipt.expectedInputHash);
    assert.equal(run.receipt.applicationDrainVerified, true);
    if (implementation === 'rust') {
      assert.equal(run.receipt.nativeTransportVerified, true);
      assert.equal(run.receipt.processed.pendingJobs, 0);
      for (const key of ['unknown', 'finite', 'ready', 'blocking']) assert.equal(run.receipt.processed.rawTasks[key], 0);
    }
    assert(run.replayWallMs > 0 && Number.isFinite(run.replayWallMs));
    for (const key of ['user', 'system', 'total']) assert(run.replayCpuMs[key] >= 0 && Number.isFinite(run.replayCpuMs[key]));
    assert(run.replayCpuMs.total > 0, run.label + ' missing positive CPU delta');
  }
}
const stats = values => ({ samples: values, median: (values[0] + values[1]) / 2, min: Math.min(...values), max: Math.max(...values) });
const profile = runs => ({ cpuMs: stats(runs.map(run => run.replayCpuMs.total)), userCpuMs: stats(runs.map(run => run.replayCpuMs.user)), systemCpuMs: stats(runs.map(run => run.replayCpuMs.system)), wallMs: stats(runs.map(run => run.replayWallMs)) });
const [control, candidate] = bySide.map(profile);
const ts = reports.map(report => profile(report.runs.filter(run => run.implementation === 'ts')));
console.log(JSON.stringify({ sampleCountQualification: 'One approved full pair, two native repeats per side; no three-pair inference.', paths, controls, graphs: graphs.map(({ value, ...item }) => item), control, candidate, ts, cpuSpeedup: control.cpuMs.median / candidate.cpuMs.median, candidateToTsCpu: candidate.cpuMs.median / ts[1].cpuMs.median, candidateToTsWall: candidate.wallMs.median / ts[1].wallMs.median, targetCpuWithinTwoTimesTs: candidate.cpuMs.median <= 2 * ts[1].cpuMs.median }, null, 2));
assert(candidate.cpuMs.median < control.cpuMs.median, 'Selected lookup must reduce CPU on the matched full tape');
