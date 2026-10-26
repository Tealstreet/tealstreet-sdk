const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const assert = require('node:assert/strict');

const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const inputOf = ({ expected, ...input }) => input;
const inputsSha256 = cases => sha256(JSON.stringify(cases.map(inputOf)));
const encodedNumber = value => typeof value === 'string' &&
  (['NaN', 'Infinity', '-Infinity', '-0'].includes(value) ||
   (Number.isFinite(Number(value)) && String(Number(value)) === value));

// Regenerate the canonical corpus from a real local oracle. Only measured
// disagreements are substituted; hashes still authenticate every common value.
function regenerateBaseline({ bytes, configuration }, disagreementsBytes, pins) {
  assert.ok(Object.hasOwn(pins.configurations, configuration), `unmeasured configuration ${configuration}`);
  const disagreements = JSON.parse(disagreementsBytes);
  assert.ok(disagreements.configurations.includes(pins.baselineConfiguration), 'invalid baseline configuration');
  const cases = JSON.parse(bytes);
  const byIndex = new Map(disagreements.cases.map(row => [row.index, row]));
  return Buffer.from(JSON.stringify(cases.map((row, index) => byIndex.has(index)
    ? { ...row, expected: byIndex.get(index).observed[pins.baselineConfiguration] }
    : row)));
}

// Hashes protect contents; these semantic checks protect the meaning of the split.
function validate({ baselineBytes, disagreementsBytes, pins, live = [] }) {
  assert.equal(sha256(baselineBytes), pins.baselineSha256, 'baseline SHA changed');
  assert.equal(sha256(disagreementsBytes), pins.disagreementsSha256, 'disagreements SHA changed');
  const baseline = JSON.parse(baselineBytes);
  const disagreements = JSON.parse(disagreementsBytes);
  assert.ok(Array.isArray(baseline), 'baseline must be an array');
  assert.equal(baseline.length, pins.count, 'corpus count changed');
  assert.equal(inputsSha256(baseline), pins.inputsSha256, 'input SHA changed');
  assert.ok(baseline.every(row => encodedNumber(row.expected)), 'invalid baseline expected');
  const configurations = disagreements.configurations;
  assert.ok(Array.isArray(configurations) && configurations.length >= 2, 'need measured configurations');
  assert.ok(configurations.includes(pins.baselineConfiguration), 'invalid baseline configuration');
  assert.equal(new Set(configurations).size, configurations.length, 'duplicate configuration');
  assert.deepEqual([...configurations].sort(), Object.keys(pins.configurations).sort(), 'configuration coverage');
  for (const configuration of configurations) {
    assert.match(configuration, /^node-v\d+\.\d+\.\d+-[a-z0-9]+-[a-z0-9]+-(standard|legacy)$/);
    assert.ok(['standard', 'legacy'].includes(pins.configurations[configuration].mode), 'invalid mode');
    assert.ok(configuration.endsWith('-' + pins.configurations[configuration].mode), 'mode mismatch');
    assert.match(pins.configurations[configuration].sha256, /^[a-f0-9]{64}$/);
  }
  assert.ok(Array.isArray(disagreements.cases), 'disagreement cases must be array');
  const byIndex = new Map();
  let previous = -1;
  for (const row of disagreements.cases) {
    assert.ok(Number.isInteger(row.index) && row.index >= 0 && row.index < baseline.length, 'orphan disagreement');
    assert.ok(!byIndex.has(row.index), 'duplicate disagreement');
    assert.ok(row.index > previous, 'disagreements must be ordered');
    previous = row.index;
    assert.deepEqual(row.input, inputOf(baseline[row.index]), 'disagreement input mismatch');
    assert.deepEqual(Object.keys(row.observed).sort(), [...configurations].sort(), 'observation coverage');
    const values = Object.values(row.observed);
    assert.ok(values.every(encodedNumber), 'invalid observed number');
    assert.ok(new Set(values).size > 1, 'listed case must disagree');
    assert.ok(values.includes(baseline[row.index].expected), 'baseline absent from observations');
    assert.equal(baseline[row.index].expected, row.observed[pins.baselineConfiguration], 'baseline configuration value mismatch');
    byIndex.set(row.index, row);
  }
  // Every measured configuration must be reconstructible, even on another host.
  // This detects omitted disagreements and corrupt inactive-host observations.
  for (const configuration of configurations) {
    const reconstructed = baseline.map((row, index) => byIndex.has(index)
      ? { ...row, expected: byIndex.get(index).observed[configuration] }
      : row);
    assert.equal(sha256(JSON.stringify(reconstructed)), pins.configurations[configuration].sha256,
      `reconstructed corpus SHA changed: ${configuration}`);
  }
  const seen = new Set();
  for (const { configuration, bytes } of live) {
    assert.ok(Object.hasOwn(pins.configurations, configuration), `unmeasured configuration ${configuration}`);
    assert.ok(!seen.has(configuration), 'duplicate live configuration');
    seen.add(configuration);
    assert.equal(sha256(bytes), pins.configurations[configuration].sha256, `live corpus SHA changed: ${configuration}`);
    const cases = JSON.parse(bytes);
    assert.equal(cases.length, pins.count, 'live corpus count changed');
    assert.equal(inputsSha256(cases), pins.inputsSha256, 'live input SHA changed');
    for (let index = 0; index < cases.length; index++) {
      const row = byIndex.get(index);
      assert.equal(cases[index].expected, row ? row.observed[configuration] : baseline[index].expected,
        `live expected changed at ${index}: ${configuration}`);
    }
  }
  return baseline.map((row, index) => byIndex.has(index)
    ? { ...row, observed: [...new Set(Object.values(byIndex.get(index).observed))] }
    : row);
}

function main() {
  const directory = path.join(__dirname, 'corpus');
  const disagreementsBytes = fs.readFileSync(path.join(directory, 'engines-disagree.json'));
  const pins = JSON.parse(fs.readFileSync(path.join(directory, 'pins.json')));
  const live = [];
  for (const [mode, flag] of [['standard', '--use-std-math-pow'], ['legacy', '--no-use-std-math-pow']]) {
    const configuration = `node-${process.version}-${process.platform}-${process.arch}-${mode}`;
    assert.ok(Object.hasOwn(pins.configurations, configuration), `unmeasured configuration ${configuration}`);
    const result = spawnSync(process.execPath, [flag, path.join(__dirname, 'oracle.cjs')], { maxBuffer: 128 * 1024 * 1024 });
    assert.equal(result.status, 0, String(result.stderr || result.error));
    live.push({ configuration, bytes: result.stdout });
  }
  const baselineBytes = regenerateBaseline(live[0], disagreementsBytes, pins);
  process.stdout.write(JSON.stringify(validate({ baselineBytes, disagreementsBytes, pins, live })));
}
module.exports = { validate, regenerateBaseline, sha256, inputsSha256 };
if (require.main === module) main();
