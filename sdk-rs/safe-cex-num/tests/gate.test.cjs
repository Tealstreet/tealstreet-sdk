const { test } = require('node:test');
const assert = require('node:assert/strict');
const { validate, regenerateBaseline, sha256, inputsSha256 } = require('./gate.cjs');
const ids = ['node-v24.20.0-darwin-arm64-standard', 'node-v24.20.0-darwin-arm64-legacy'];
function fixture(change = () => {}) {
  const baseline = [{ op: 'add', a: '1', b: '2', expected: '3' },
    { op: 'multiply', a: '1e-24', b: '0.01', expected: '9.999999999999999e-27' }];
  const disagreements = { configurations: ids, cases: [{ index: 1,
    input: { op: 'multiply', a: '1e-24', b: '0.01' },
    observed: { [ids[0]]: baseline[1].expected, [ids[1]]: '1.0000000000000002e-26' } }] };
  const liveCases = baseline.map(row => ({ ...row }));
  const recorded = {
    [ids[0]]: baseline.map(row => ({ ...row })),
    [ids[1]]: baseline.map((row, index) => index === 1
      ? { ...row, expected: disagreements.cases[0].observed[ids[1]] } : { ...row }),
  };
  change({ baseline, disagreements, liveCases });
  const baselineBytes = Buffer.from(JSON.stringify(baseline));
  const disagreementsBytes = Buffer.from(JSON.stringify(disagreements));
  const bytes = Buffer.from(JSON.stringify(liveCases));
  const pins = { baselineConfiguration: ids[0], count: baseline.length, baselineSha256: sha256(baselineBytes),
    disagreementsSha256: sha256(disagreementsBytes), inputsSha256: inputsSha256(baseline),
    configurations: Object.fromEntries(ids.map(id => [id, { sha256: sha256(JSON.stringify(recorded[id])), mode: id.endsWith('standard') ? 'standard' : 'legacy' }])) };
  return { baselineBytes, disagreementsBytes, pins, live: [{ configuration: ids[0], bytes }] };
}
test('only genuine disagreement rows receive an allowance', () => {
  const rows = validate(fixture());
  assert.equal(rows[0].observed, undefined);
  assert.deepEqual(rows[1].observed, ['9.999999999999999e-27', '1.0000000000000002e-26']);
});
test('same-count changed input fails content pin', () => {
  const f = fixture();
  f.baselineBytes = Buffer.from(f.baselineBytes.toString().replace('"a":"1"', '"a":"4"'));
  assert.throws(() => validate(f), /baseline SHA changed/);
});
test('changed common expected fails content pin', () => {
  const f = fixture();
  f.baselineBytes = Buffer.from(f.baselineBytes.toString().replace('"expected":"3"', '"expected":"4"'));
  assert.throws(() => validate(f), /baseline SHA changed/);
});
test('orphan disagreement fails even with repinned contents', () => {
  assert.throws(() => validate(fixture(({ disagreements }) => { disagreements.cases[0].index = 2; })), /orphan/);
});
test('duplicate disagreement fails even with repinned contents', () => {
  assert.throws(() => validate(fixture(({ disagreements }) => { disagreements.cases.push(disagreements.cases[0]); })), /duplicate disagreement/);
});
test('missing observed configuration fails', () => {
  assert.throws(() => validate(fixture(({ disagreements }) => { delete disagreements.cases[0].observed[ids[1]]; })), /observation coverage/);
});
test('live common divergence fails content pin', () => {
  assert.throws(() => validate(fixture(({ liveCases }) => { liveCases[0].expected = '3.0000000000000004'; })), /live corpus SHA changed/);
});
test('live input mutation fails content pin', () => {
  assert.throws(() => validate(fixture(({ liveCases }) => { liveCases[0].a = '4'; })), /live corpus SHA changed/);
});
test('unmeasured configuration fails closed', () => {
  const f = fixture();
  f.live[0].configuration = 'node-v24.20.0-linux-x64-standard';
  assert.throws(() => validate(f), /unmeasured configuration/);
});
test('observations must include baseline and actually disagree', () => {
  assert.throws(() => validate(fixture(({ disagreements }) => { disagreements.cases[0].observed[ids[1]] = disagreements.cases[0].observed[ids[0]]; })), /must disagree/);
  assert.throws(() => validate(fixture(({ disagreements }) => { disagreements.cases[0].observed[ids[0]] = '2'; })), /baseline absent/);
});

test('omitted disagreement fails reconstructed inactive configuration hash', () => {
  assert.throws(() => validate(fixture(({ disagreements }) => { disagreements.cases = []; })), /reconstructed corpus SHA changed/);
});
test('corrupt inactive configuration observation fails reconstructed hash', () => {
  assert.throws(() => validate(fixture(({ disagreements }) => { disagreements.cases[0].observed[ids[1]] = '1e-26'; })), /reconstructed corpus SHA changed/);
});

test('both live engine modes regenerate identical canonical corpus bytes', () => {
  const f = fixture();
  const standard = f.live[0];
  const legacyCases = JSON.parse(standard.bytes);
  legacyCases[1].expected = JSON.parse(f.disagreementsBytes).cases[0].observed[ids[1]];
  const legacy = { configuration: ids[1], bytes: Buffer.from(JSON.stringify(legacyCases)) };
  assert.deepEqual(regenerateBaseline(standard, f.disagreementsBytes, f.pins), f.baselineBytes);
  assert.deepEqual(regenerateBaseline(legacy, f.disagreementsBytes, f.pins), f.baselineBytes);
  assert.doesNotThrow(() => validate({ ...f, baselineBytes: regenerateBaseline(legacy, f.disagreementsBytes, f.pins), live: [legacy] }));
});
test('missing disagreement during regeneration fails canonical corpus SHA', () => {
  const f = fixture();
  const legacyCases = JSON.parse(f.live[0].bytes);
  legacyCases[1].expected = JSON.parse(f.disagreementsBytes).cases[0].observed[ids[1]];
  const legacy = { configuration: ids[1], bytes: Buffer.from(JSON.stringify(legacyCases)) };
  f.disagreementsBytes = Buffer.from(JSON.stringify({ configurations: ids, cases: [] }));
  f.pins.disagreementsSha256 = sha256(f.disagreementsBytes);
  f.baselineBytes = regenerateBaseline(legacy, f.disagreementsBytes, f.pins);
  assert.throws(() => validate(f), /baseline SHA changed/);
});
test('unknown canonical anchor fails validation and regeneration', () => {
  const f = fixture();
  f.pins.baselineConfiguration = 'unknown';
  assert.throws(() => validate(f), /invalid baseline configuration/);
  assert.throws(() => regenerateBaseline(f.live[0], f.disagreementsBytes, f.pins), /invalid baseline configuration/);
});
