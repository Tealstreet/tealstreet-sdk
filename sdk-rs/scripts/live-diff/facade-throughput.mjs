import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {deserialize} from 'node:v8';
const [beforePath, afterPath] = process.argv.slice(2);
const before = JSON.parse(readFileSync(beforePath));
const after = JSON.parse(readFileSync(afterPath));
for (const report of [before, after]) {
  assert.equal(report.nativeDrainVerified, true);
  assert.equal(report.comparisonsProvisional, false);
  assert.equal(report.capture.topics['markPriceUpdate'] > 0, true);
}
const native = report => report.runs.filter(run => run.implementation === 'rust');
const ts = report => report.runs.filter(run => run.implementation === 'ts');
const baseline = native(before);
const candidate = native(after);
assert.equal(baseline.length, 2);
assert.equal(candidate.length, 2);
const expected = deserialize(readFileSync(baseline[0].fixture));
for (const run of [...baseline, ...candidate]) {
  assert.equal(run.receipt.delivered, 570);
  assert.equal(run.receipt.dropped, 0);
  assert.equal(run.receipt.inputHash, baseline[0].receipt.inputHash);
  assert.equal(run.receipt.applicationDrainVerified, true);
  assert.equal(run.receipt.nativeTransportVerified, true);
  assert.deepStrictEqual(deserialize(readFileSync(run.fixture)), expected,
    `${run.label} complete retained output/state must equal baseline without normalization`);
}
const mean = runs => runs.reduce((sum, run) => sum + run.replayWallMs, 0) / runs.length;
console.log(JSON.stringify({
  identicalCompleteNativeOutput: true,
  inputHash: baseline[0].receipt.inputHash,
  baseline: baseline.map(run => run.replayWallMs),
  candidate: candidate.map(run => run.replayWallMs),
  tsBaseline: ts(before).map(run => run.replayWallMs),
  tsCandidate: ts(after).map(run => run.replayWallMs),
  speedup: mean(baseline) / mean(candidate),
  candidateToTs: mean(candidate) / mean(ts(after)),
  targetWithinTwoTimesTs: mean(candidate) <= 2 * mean(ts(after)),
}, null, 2));
