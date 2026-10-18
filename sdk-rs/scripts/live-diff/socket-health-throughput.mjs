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
    assert(report.capture.topics.markPriceUpdate > 0);
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
// The health fix intentionally adds source-owned undefined properties. Compare
// that exact delta in place; never sanitize or normalize either retained output.
const optionalHealth = ['publicLastMessage', 'privateLastMessage', 'publicCloseCode', 'privateCloseCode'];
const sourceHealthOrder = [
  'publicHealthy',
  'privateHealthy',
  'publicInitialized',
  'privateInitialized',
  'publicLastMessage',
  'privateLastMessage',
  'publicCloseCode',
  'publicCloseCountTotal',
  'publicCloseStreak',
  'publicReconnectPaused',
  'privateCloseCode',
  'privateCloseCountTotal',
  'privateCloseStreak',
  'privateReconnectPaused',
];
let healthRecords = 0,
  undefinedAdditions = 0;
function healthDelta(control, candidate, path) {
  if (/^\$\.(?:state|outputs\.\d+\.data)\.socketHealth$/.test(path) && candidate && typeof candidate === 'object') {
    healthRecords++;
    const keys = Object.keys(candidate);
    if (keys.some((key) => optionalHealth.includes(key))) {
      assert.deepStrictEqual(keys, sourceHealthOrder, path + ' source property order');
    }
    for (const key of optionalHealth) {
      if (Object.hasOwn(control, key)) {
        assert(Object.hasOwn(candidate, key), path + '.' + key + ' was deleted');
        assert.deepStrictEqual(candidate[key], control[key], path + '.' + key + ' value');
      } else if (Object.hasOwn(candidate, key)) {
        assert.equal(candidate[key], undefined, path + '.' + key + ' must be own undefined');
        undefinedAdditions++;
      }
    }
    assert.deepStrictEqual(
      Object.keys(control).filter((key) => !optionalHealth.includes(key)),
      keys.filter((key) => !optionalHealth.includes(key)),
      path + ' other own properties',
    );
    for (const key of Object.keys(control)) {
      if (!optionalHealth.includes(key)) healthDelta(control[key], candidate[key], path + '.' + key);
    }
    return;
  }
  if (!control || typeof control !== 'object') {
    assert.deepStrictEqual(candidate, control, path);
    return;
  }
  assert(candidate && typeof candidate === 'object', path + ' object shape');
  assert.equal(Object.getPrototypeOf(candidate), Object.getPrototypeOf(control), path + ' prototype');
  assert.equal(Array.isArray(candidate), Array.isArray(control), path + ' array shape');
  if (Array.isArray(control)) assert.equal(candidate.length, control.length, path + ' array length');
  assert.deepStrictEqual(Reflect.ownKeys(candidate), Reflect.ownKeys(control), path + ' own properties/order');
  for (const key of Reflect.ownKeys(control)) healthDelta(control[key], candidate[key], path + '.' + String(key));
}
const controlOutput = deserialize(readFileSync(baseline[0].fixture));
const candidateOutput = deserialize(readFileSync(candidate[0].fixture));
healthDelta(controlOutput, candidateOutput, '$');
assert(healthRecords > 0 && undefinedAdditions > 0, 'The recorded tape must observe the intended health delta');
for (const run of [...baseline, ...candidate]) {
  assert.equal(run.receipt.delivered, 570);
  assert.equal(run.receipt.dropped, 0);
  assert.equal(run.receipt.inputHash, baseline[0].receipt.inputHash);
  assert.equal(run.receipt.applicationDrainVerified, true);
  assert.equal(run.receipt.nativeTransportVerified, true);
  assert.deepStrictEqual(
    deserialize(readFileSync(run.fixture)),
    baseline.includes(run) ? controlOutput : candidateOutput,
    run.label + ' complete retained output/state must repeat within its side without normalization',
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
const health = profile(candidate);
const tsBefore = profile(runs(before, 'ts'));
const tsAfter = profile(runs(after, 'ts'));
console.log(
  JSON.stringify(
    {
      completeNativeOutputMatchesHealthContract: true,
      healthRecords,
      undefinedAdditions,
      allowedHealthProperties: optionalHealth,
      sourceHealthOrder,
      timingHarnessCommit: 'e5da2bf716eab49d0b9d7ddacf68df25b549a35c',
      inputHash: baseline[0].receipt.inputHash,
      interleavedReportPaths: paths,
      control,
      health,
      tsBefore,
      tsAfter,
      cpuSpeedup: control.cpuMs.median / health.cpuMs.median,
      wallSpeedup: control.wallMs.median / health.wallMs.median,
      candidateToTsCpu: health.cpuMs.median / tsAfter.cpuMs.median,
      candidateToTsWall: health.wallMs.median / tsAfter.wallMs.median,
    },
    null,
    2,
  ),
);
