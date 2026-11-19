// Rewrites wasm-caps.json from one CI run's sealed family artifacts (and its combined
// package) to rule 17: cap = max(current, ceil((measured + 10 KiB) / 1 KiB) KiB).
// A cap is never lowered. Run by .github/workflows/wasm-caps.yml after master CI.
//   node sdk-rs/harness/update-wasm-caps.mjs --artifacts DIR --root MEASURED_CHECKOUT \
//     --source-head SHA --run-id ID [--caps FILE] [--merge FILE] [--report FILE]
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { readWasmCaps } from './artifact-census.mjs';
import { validateCiWasm } from './ci-wasm.mjs';
import { nextWasmCap, reserveTargetCap, WASM_CAPS_PATH } from './wasm-budget.mjs';

const FAMILY_PREFIX = 'wasm-family-';
const COMBINED_ARTIFACT = 'replay-combined-package';
const MODULE = 'safe_cex_wasm_bg.wasm';
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

const readModule = (path) => {
  if (!existsSync(path)) throw new Error(`missing measured module: ${path}`);
  const bytes = readFileSync(path);
  return { bytes: bytes.length, sha256: sha256(bytes) };
};

// The size is the larger of the web and Node modules; the Node module must be the
// exact sealed one, so the measurement is bound to the validated build contract.
export function measureFamilyArtifact({ root, directory, family, sourceHead, runId, validate = validateCiWasm }) {
  const { metadata } = validate({ root, directory: join(directory, 'ci-replay'), venue: family });
  if (metadata.family !== family) throw new Error(`${FAMILY_PREFIX}${family} sealed ${metadata.family}`);
  if (sourceHead !== undefined && metadata.sourceHead !== sourceHead) throw new Error(`${family}: sealed source differs from the measured run`);
  if (runId !== undefined && metadata.runId !== runId) throw new Error(`${family}: sealed run differs from the measured run`);
  const node = readModule(join(directory, 'node', family, MODULE));
  const web = readModule(join(directory, 'web', family, MODULE));
  if (node.sha256 !== metadata.hashes?.[MODULE]) throw new Error(`${family}: Node module differs from its sealed build`);
  return { name: family, bytes: Math.max(node.bytes, web.bytes), node, web };
}

// The combined package carries no seal; it is trusted only as an artifact of the same run.
export function measureCombinedArtifact(directory) {
  const node = readModule(join(directory, 'node', MODULE));
  const web = readModule(join(directory, 'web', MODULE));
  return { name: 'combined', bytes: Math.max(node.bytes, web.bytes), node, web };
}

export function collectMeasurements({ artifacts, root, sourceHead, runId, validate }) {
  const measurements = [];
  for (const entry of readdirSync(artifacts, { withFileTypes: true }).filter((entry) => entry.isDirectory()).sort((a, b) => a.name.localeCompare(b.name))) {
    const directory = join(artifacts, entry.name);
    if (entry.name.startsWith(FAMILY_PREFIX)) {
      measurements.push(measureFamilyArtifact({ root, directory, family: entry.name.slice(FAMILY_PREFIX.length), sourceHead, runId, validate }));
    } else if (entry.name === COMBINED_ARTIFACT) {
      measurements.push(measureCombinedArtifact(directory));
    } else {
      throw new Error(`unexpected artifact directory: ${entry.name}`);
    }
  }
  if (!measurements.some(({ name }) => name !== 'combined')) throw new Error('no sealed family artifacts to measure');
  return measurements;
}

const capOf = (caps, name) => (name === 'combined' ? caps.combined : caps.families[name]);

// Families the run did not measure keep their cap; an unknown family is refused,
// since adding one is a reviewed census change, not a bot edit.
export function planWasmCaps(caps, measurements, mergeWith = []) {
  const next = { ...caps, families: { ...caps.families } };
  for (const other of mergeWith) {
    next.combined = Math.max(next.combined, other.combined);
    for (const [name, cap] of Object.entries(other.families)) if (Object.hasOwn(next.families, name)) next.families[name] = Math.max(next.families[name], cap);
  }
  const seen = new Set();
  const rows = [];
  for (const { name, bytes } of measurements) {
    if (seen.has(name)) throw new Error(`duplicate measurement: ${name}`);
    seen.add(name);
    if (name !== 'combined' && !Object.hasOwn(next.families, name)) throw new Error(`unknown wasm family: ${name}`);
    const before = capOf(caps, name);
    const after = nextWasmCap(capOf(next, name), bytes, caps.reserveBytes);
    if (name === 'combined') next.combined = after;
    else next.families[name] = after;
    rows.push({ name, bytes, before, target: reserveTargetCap(bytes, caps.reserveBytes), after, reserve: after - bytes });
  }
  return { caps: next, rows, changed: serializeWasmCaps(next) !== serializeWasmCaps(caps) };
}

export const serializeWasmCaps = (caps) => `${JSON.stringify({
  schema: caps.schema,
  reserveBytes: caps.reserveBytes,
  combined: caps.combined,
  families: Object.fromEntries(Object.keys(caps.families).sort().map((name) => [name, caps.families[name]])),
}, null, 2)}\n`;

export function formatCapsTable(rows) {
  const lines = ['| module | measured B | cap before | rule-17 target | cap after | reserve B |', '| --- | ---: | ---: | ---: | ---: | ---: |'];
  for (const row of rows) {
    const after = row.after === row.before ? `${row.after}` : `**${row.after}** (+${row.after - row.before})`;
    lines.push(`| ${row.name} | ${row.bytes} | ${row.before} | ${row.target} | ${after} | ${row.reserve} |`);
  }
  return `${lines.join('\n')}\n`;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: {
    artifacts: { type: 'string' }, root: { type: 'string' }, 'source-head': { type: 'string' }, 'run-id': { type: 'string' },
    caps: { type: 'string' }, merge: { type: 'string', multiple: true }, report: { type: 'string' },
  } });
  if (!values.artifacts || !values.root || !/^[a-f0-9]{40}$/.test(values['source-head'] ?? '') || !/^\d+$/.test(values['run-id'] ?? ''))
    throw new Error('Expected --artifacts DIR --root CHECKOUT --source-head SHA --run-id ID');
  const capsPath = resolve(values.caps ?? WASM_CAPS_PATH);
  const caps = readWasmCaps(readFileSync(capsPath, 'utf8'));
  const mergeWith = (values.merge ?? []).filter((path) => existsSync(path)).map((path) => readWasmCaps(readFileSync(path, 'utf8')));
  // Validate with the measured tree's own ci-wasm.mjs: its build-input list defines
  // the fingerprint that run sealed, and master's list may since have grown.
  const root = resolve(values.root);
  const { validateCiWasm: validate } = await import(pathToFileURL(join(root, 'sdk-rs/harness/ci-wasm.mjs')).href);
  const measurements = collectMeasurements({ artifacts: resolve(values.artifacts), root,
    sourceHead: values['source-head'], runId: values['run-id'], validate });
  const plan = planWasmCaps(caps, measurements, mergeWith);
  const table = formatCapsTable(plan.rows);
  console.log(table);
  if (values.report) writeFileSync(values.report, table);
  if (plan.changed) writeFileSync(capsPath, serializeWasmCaps(plan.caps));
  console.log(plan.changed ? `Raised caps written to ${capsPath}` : 'Every measured module already keeps its rule-17 reserve');
}
