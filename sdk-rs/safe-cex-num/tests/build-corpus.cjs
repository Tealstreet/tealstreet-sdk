const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const input = ({ expected, ...tuple }) => tuple;
const directories = process.argv.slice(2);
if (!directories.length) throw new Error('Pass directories of actual measure.cjs observations');
const observations = directories.flatMap(directory => fs.readdirSync(directory).sort().filter(name => /^node-v.*\.json$/.test(name)).map(name => {
  const bytes = fs.readFileSync(path.join(directory, name));
  return { id: name.slice(0, -5), bytes, cases: JSON.parse(bytes) };
})).sort((a, b) => a.id.localeCompare(b.id, 'en'));
if (new Set(observations.map(o => o.id)).size !== observations.length) throw new Error('Duplicate configuration');
const baseline = observations[0];
if (!baseline || baseline.cases.length !== 476804) throw new Error('Unexpected corpus count');
const inputsSha256 = sha(JSON.stringify(baseline.cases.map(input)));
for (const observation of observations) {
  if (observation.cases.length !== baseline.cases.length || sha(JSON.stringify(observation.cases.map(input))) !== inputsSha256) throw new Error('Inputs differ: ' + observation.id);
}
const disagreements = { configurations: observations.map(o => o.id), cases: [] };
for (let index = 0; index < baseline.cases.length; index++) {
  const observed = Object.fromEntries(observations.map(o => [o.id, o.cases[index].expected]));
  if (new Set(Object.values(observed)).size > 1) disagreements.cases.push({ index, input: input(baseline.cases[index]), observed });
}
const disagreementBytes = Buffer.from(JSON.stringify(disagreements) + '\n');
const pins = {
  count: baseline.cases.length,
  baselineConfiguration: baseline.id,
  baselineSha256: sha(baseline.bytes),
  disagreementsSha256: sha(disagreementBytes),
  inputsSha256,
  configurations: Object.fromEntries(observations.map(o => [o.id, { sha256: sha(o.bytes), mode: o.id.endsWith('-standard') ? 'standard' : 'legacy' }]))
};
const directory = path.join(__dirname, 'corpus');
fs.mkdirSync(directory, { recursive: true });
fs.writeFileSync(path.join(directory, 'engines-disagree.json'), disagreementBytes);
fs.writeFileSync(path.join(directory, 'pins.json'), JSON.stringify(pins, null, 2) + '\n');
console.log({ configurations: observations.map(o => o.id), common: baseline.cases.length - disagreements.cases.length, disagreements: disagreements.cases.length });
