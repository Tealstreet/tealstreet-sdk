// Full original OKX tape; unmodified streams runner and complete retained output.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { deserialize } from 'node:v8';

const paths = process.argv.slice(2);
assert(
  paths.length >= 6 && paths.length % 2 === 0,
  'Supply interleaved control/candidate reports, at least three of each',
);
const reports = paths.map((path) => JSON.parse(readFileSync(path)));
const before = reports.filter((_, i) => i % 2 === 0);
const after = reports.filter((_, i) => i % 2 === 1);
const runs = (group, implementation) =>
  group.flatMap((report) => report.runs.filter((run) => run.implementation === implementation));
const baseline = runs(before, 'rust');
const candidate = runs(after, 'rust');
assert.equal(baseline.length, before.length * 2);
assert.equal(candidate.length, after.length * 2);
for (const group of [before, after]) {
  for (const report of group) {
    assert.equal(report.addonSha256, group[0].addonSha256, 'Each side must use one immutable addon');
    assert.equal(report.rustRevision, group[0].rustRevision);
    assert.equal(report.nativeDrainVerified, true);
    assert.equal(report.comparisonsProvisional, false);
    assert.equal(report.capture.frames, 285713);
    assert(report.capture.topics['index-tickers'] > 0);
  }
}
assert.notEqual(before[0].addonSha256, after[0].addonSha256, 'Control and candidate must be different artifacts');
for (let i = 1; i < reports.length; i++) {
  assert(
    Date.parse(reports[i].at) >= Date.parse(reports[i - 1].at),
    'Reports must be supplied in actual interleaved run order',
  );
}
assert.equal(runs(before, 'ts').length, before.length * 2);
assert.equal(runs(after, 'ts').length, after.length * 2);
const expectedBytes = readFileSync(baseline[0].fixture);
const expected = deserialize(expectedBytes);
for (const run of [...baseline, ...candidate]) {
  assert.equal(run.receipt.delivered, 285713);
  assert.equal(run.receipt.dropped, 0);
  assert.equal(run.receipt.inputHash, baseline[0].receipt.inputHash);
  assert.equal(run.receipt.inputHash, run.receipt.expectedInputHash);
  assert.equal(run.receipt.applicationDrainVerified, true);
  assert.equal(run.receipt.nativeTransportVerified, true);
  const actualBytes = readFileSync(run.fixture);
  assert.deepStrictEqual(
    deserialize(actualBytes),
    expected,
    run.label + ' complete retained output/state must equal baseline without normalization',
  );
  assert(
    actualBytes.equals(expectedBytes),
    run.label + ' serialized property order and reference structure must be identical',
  );
}
for (const report of reports) {
  for (const run of report.runs) {
    assert(Number.isFinite(run.replayWallMs) && run.replayWallMs > 0);
    for (const key of ['user', 'system', 'total']) {
      assert(
        Number.isFinite(run.replayCpuMs?.[key]) && run.replayCpuMs[key] >= 0,
        run.label + ' missing or invalid process CPU delta: ' + key,
      );
    }
    assert(run.replayCpuMs.total > 0);
  }
}
const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};
const stats = (values) => ({
  median: median(values),
  min: Math.min(...values),
  max: Math.max(...values),
  samples: values,
});
const profile = (values) => ({
  cpuMs: stats(values.map((run) => run.replayCpuMs.total)),
  userCpuMs: stats(values.map((run) => run.replayCpuMs.user)),
  systemCpuMs: stats(values.map((run) => run.replayCpuMs.system)),
  wallMs: stats(values.map((run) => run.replayWallMs)),
});
const control = profile(baseline);
const lookup = profile(candidate);
const tsBefore = profile(runs(before, 'ts'));
const tsAfter = profile(runs(after, 'ts'));
console.log(
  JSON.stringify(
    {
      identicalCompleteNativeOutput: true,
      identicalSerializedNativeOutput: true,
      inputHash: baseline[0].receipt.inputHash,
      interleavedReportPaths: paths,
      control,
      lookup,
      tsBefore,
      tsAfter,
      cpuSpeedup: control.cpuMs.median / lookup.cpuMs.median,
      wallSpeedup: control.wallMs.median / lookup.wallMs.median,
      candidateToTsCpu: lookup.cpuMs.median / tsAfter.cpuMs.median,
      candidateToTsWall: lookup.wallMs.median / tsAfter.wallMs.median,
      targetCpuWithinTwoTimesTs: lookup.cpuMs.median <= 2 * tsAfter.cpuMs.median,
      targetWallWithinTwoTimesTs: lookup.wallMs.median <= 2 * tsAfter.wallMs.median,
    },
    null,
    2,
  ),
);
