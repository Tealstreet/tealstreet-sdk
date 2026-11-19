import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { readWasmCaps } from './artifact-census.mjs';
import { collectMeasurements, formatCapsTable, planWasmCaps, serializeWasmCaps } from './update-wasm-caps.mjs';
import { WASM_CAPS_PATH } from './wasm-budget.mjs';

const KiB = 1024;
const SOURCE = 'a'.repeat(40);
const RUN = '123';
const current = () => readWasmCaps(readFileSync(WASM_CAPS_PATH, 'utf8'));
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

test('a measured module short of the reserve is raised to measured + 10 KiB, rounded up', () => {
  const caps = current();
  const measured = caps.families.okx - 5;
  const plan = planWasmCaps(caps, [{ name: 'okx', bytes: measured }]);
  assert.equal(plan.changed, true);
  assert.equal(plan.caps.families.okx, Math.ceil((measured + 10 * KiB) / KiB) * KiB);
  assert.deepEqual(plan.rows, [{ name: 'okx', bytes: measured, before: caps.families.okx,
    target: plan.caps.families.okx, after: plan.caps.families.okx, reserve: plan.caps.families.okx - measured }]);
  for (const family of Object.keys(caps.families).filter((name) => name !== 'okx'))
    assert.equal(plan.caps.families[family], caps.families[family], `${family} unmeasured, unchanged`);
});

test('a smaller module never lowers its cap and leaves the file unchanged', () => {
  const caps = current();
  const plan = planWasmCaps(caps, Object.entries(caps.families).map(([name, cap]) => ({ name, bytes: cap - 100 * KiB })));
  assert.equal(plan.changed, false);
  assert.deepEqual(plan.caps, caps);
  assert.ok(plan.rows.every((row) => row.after === row.before && row.target < row.before));
});

test('the combined package follows the same rule', () => {
  const caps = current();
  const plan = planWasmCaps(caps, [{ name: 'combined', bytes: caps.combined }]);
  assert.equal(plan.caps.combined, caps.combined + 10 * KiB);
  assert.equal(planWasmCaps(caps, [{ name: 'combined', bytes: caps.combined - 20 * KiB }]).changed, false);
});

test('unknown or duplicate measurements refuse instead of growing the census', () => {
  const caps = current();
  assert.throws(() => planWasmCaps(caps, [{ name: 'newvenue', bytes: 1 }]), /unknown wasm family: newvenue/);
  assert.throws(() => planWasmCaps(caps, [{ name: 'okx', bytes: 1 }, { name: 'okx', bytes: 2 }]), /duplicate measurement: okx/);
});

test('raises already proposed on the bot branch are kept, never lowered', () => {
  const caps = current();
  const proposed = { ...caps, combined: caps.combined + KiB, families: { ...caps.families, woo: caps.families.woo + 2 * KiB, okx: caps.families.okx - KiB } };
  const plan = planWasmCaps(caps, [{ name: 'bybit', bytes: caps.families.bybit }], [proposed]);
  assert.equal(plan.caps.families.woo, caps.families.woo + 2 * KiB);
  assert.equal(plan.caps.families.okx, caps.families.okx);
  assert.equal(plan.caps.combined, caps.combined + KiB);
  assert.equal(plan.caps.families.bybit, caps.families.bybit + 10 * KiB);
});

test('serialization is canonical and round-trips through the census reader', () => {
  const caps = current();
  const shuffled = { ...caps, families: Object.fromEntries(Object.entries(caps.families).reverse()) };
  assert.equal(serializeWasmCaps(shuffled), readFileSync(WASM_CAPS_PATH, 'utf8'));
  assert.deepEqual(readWasmCaps(serializeWasmCaps(caps)), caps);
});

test('the table marks raised rows', () => {
  const table = formatCapsTable([{ name: 'okx', bytes: 1, before: 11 * KiB, target: 11 * KiB, after: 11 * KiB, reserve: 11 * KiB - 1 },
    { name: 'woo', bytes: 2 * KiB, before: 11 * KiB, target: 12 * KiB, after: 12 * KiB, reserve: 10 * KiB }]);
  assert.match(table, /^\| module \| measured B/);
  assert.match(table, /\| okx \| 1 \| 11264 \| 11264 \| 11264 \|/);
  assert.match(table, /\| woo \| 2048 \| 11264 \| 12288 \| \*\*12288\*\* \(\+1024\) \| 10240 \|/);
});

function artifactTree(t, families, { webExtra = 0, nodeTamper = false, combined = true } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'wasm-caps-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const [family, size] of Object.entries(families)) {
    const base = join(root, `wasm-family-${family}`);
    const node = Buffer.alloc(size, 1);
    for (const [target, bytes] of [['node', nodeTamper ? Buffer.alloc(size, 2) : node], ['web', Buffer.alloc(size + webExtra, 1)]]) {
      mkdirSync(join(base, target, family), { recursive: true });
      writeFileSync(join(base, target, family, 'safe_cex_wasm_bg.wasm'), bytes);
    }
    mkdirSync(join(base, 'ci-replay'), { recursive: true });
    writeFileSync(join(base, 'ci-replay', 'build.json'), JSON.stringify({ family, sourceHead: SOURCE, runId: RUN, hashes: { 'safe_cex_wasm_bg.wasm': sha256(node) } }));
  }
  if (combined) for (const target of ['node', 'web']) {
    mkdirSync(join(root, 'replay-combined-package', target), { recursive: true });
    writeFileSync(join(root, 'replay-combined-package', target, 'safe_cex_wasm_bg.wasm'), Buffer.alloc(target === 'web' ? 300 : 200));
  }
  return root;
}
const seal = (calls) => ({ directory, venue }) => {
  calls?.push(venue);
  return { metadata: JSON.parse(readFileSync(join(directory, 'build.json'), 'utf8')) };
};

test('measurements validate every sealed family and take the larger of web and Node', (t) => {
  const calls = [];
  const artifacts = artifactTree(t, { okx: 100, woo: 50 }, { webExtra: 7 });
  const measurements = collectMeasurements({ artifacts, root: '.', sourceHead: SOURCE, runId: RUN, validate: seal(calls) });
  assert.deepEqual(calls, ['okx', 'woo']);
  assert.deepEqual(measurements.map(({ name, bytes }) => [name, bytes]), [['combined', 300], ['okx', 107], ['woo', 57]]);
});

test('a Node module that differs from its seal, or a seal from another run, refuses', (t) => {
  assert.throws(() => collectMeasurements({ artifacts: artifactTree(t, { okx: 10 }, { nodeTamper: true }), root: '.', validate: seal() }),
    /okx: Node module differs from its sealed build/);
  const artifacts = artifactTree(t, { okx: 10 });
  assert.throws(() => collectMeasurements({ artifacts, root: '.', sourceHead: 'b'.repeat(40), validate: seal() }), /sealed source differs/);
  assert.throws(() => collectMeasurements({ artifacts, root: '.', runId: '999', validate: seal() }), /sealed run differs/);
});

test('the default validator is validateCiWasm and refuses an unsealed directory', (t) => {
  const artifacts = artifactTree(t, { okx: 10 });
  rmSync(join(artifacts, 'wasm-family-okx', 'ci-replay', 'build.json'));
  assert.throws(() => collectMeasurements({ artifacts, root: '.' }), /ENOENT|build\.json/);
});

test('stray directories and combined-only downloads refuse', (t) => {
  const stray = artifactTree(t, { okx: 10 });
  mkdirSync(join(stray, 'something-else'));
  assert.throws(() => collectMeasurements({ artifacts: stray, root: '.', validate: seal() }), /unexpected artifact directory/);
  assert.throws(() => collectMeasurements({ artifacts: artifactTree(t, {}), root: '.', validate: seal() }), /no sealed family artifacts/);
});

test('the bot workflow measures master Rust runs and only opens a reviewed PR', () => {
  const workflow = readFileSync(new URL('../../.github/workflows/wasm-caps.yml', import.meta.url), 'utf8');
  const rust = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  assert.match(rust, /^name: Rust SDK$/m);
  assert.match(workflow, /workflow_run:\n\s+workflows: \[Rust SDK\]\n\s+types: \[completed\]\n\s+branches: \[master\]/);
  assert.match(workflow, /github\.event\.workflow_run\.event == 'push'/);
  // Artifact names must match what rust.yml uploads.
  assert.match(rust, /name: wasm-family-\$\{\{ matrix\.family \}\}/);
  assert.match(rust, /name: replay-combined-package/);
  assert.match(workflow, /pattern: wasm-family-\*/);
  // A single-artifact download extracts flat, so the combined package needs its own named directory (#3689).
  assert.match(workflow, /name: replay-combined-package\n\s+path: \$\{\{ runner\.temp \}\}\/wasm-caps\/replay-combined-package\n/);
  assert.match(workflow, /run-id: \$\{\{ github\.event\.workflow_run\.id \}\}/);
  assert.match(workflow, /node sdk-rs\/harness\/update-wasm-caps\.mjs[\s\S]*--root measured[\s\S]*--source-head "\$MEASURED_SHA" --run-id "\$MEASURED_RUN"/);
  assert.match(workflow, /ref: \$\{\{ github\.event\.workflow_run\.head_sha \}\}\n\s+path: measured/);
  assert.match(workflow, /chore\(wasm\): restore rule-17 reserve/);
  assert.match(workflow, /git commit [^\n]*\n[^\n]*Measured by Rust SDK run \$RUN_URL[^\n]*-- sdk-rs\/harness\/wasm-caps\.json/);
  // Actions may not create or approve PRs in this repo (coordinator, #3686): push the bot branch only.
  assert.match(workflow, /BOT_BRANCH: bot\/wasm-caps/);
  const pushes = [...workflow.matchAll(/^\s*git push\b.*$/gm)].map(([line]) => line.trim());
  assert.deepEqual(pushes, ['git push --force-with-lease="refs/heads/$BOT_BRANCH:${BOT_LEASE:-}" origin "HEAD:refs/heads/$BOT_BRANCH"']);
  assert.match(workflow, />> "\$GITHUB_STEP_SUMMARY"/);
  assert.doesNotMatch(workflow, /\bgh\s|GH_TOKEN|pulls|pull-requests:/, 'no PR API and no PR permission');
  assert.match(workflow, /permissions:\n\s+contents: write\n\s+actions: read\n\n/);
  assert.doesNotMatch(workflow, /secrets\./, 'GITHUB_TOKEN only');
  for (const [, action] of workflow.matchAll(/uses: ([^@\s]+)@/g)) assert.match(action, /^actions\//, 'GitHub-owned actions only');
});
