import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';
import { readBitgetPairedEvidence } from './bitget-paired-artifacts.mjs';
import { BITGET_COHORT_ORDER, BITGET_PAIRED_FREEZE_SHA256, BITGET_PAIRED_PROTOCOL_SHA256, identicalBitgetShipping } from './bitget-paired-steady.mjs';

const sha = bytes => createHash('sha256').update(bytes).digest('hex');
function fixture(t) {
  const root = mkdtempSync(resolve(tmpdir(), 'bitget-paired-artifacts-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const write = (name, value) => {
    const file = resolve(root, name); mkdirSync(resolve(file, '..'), { recursive: true });
    writeFileSync(file, typeof value === 'string' || Buffer.isBuffer(value) ? value : JSON.stringify(value));
  };
  const refs = { baseline: 'a'.repeat(40), candidate: 'b'.repeat(40) };
  write('plan.json', { protocolSha256: BITGET_PAIRED_PROTOCOL_SHA256, freezeSha256: BITGET_PAIRED_FREEZE_SHA256,
    refs, candidateTree: 'c'.repeat(40), runId: '10', runAttempt: 1, jobId: '20', runnerId: '30',
    order: BITGET_COHORT_ORDER, nodeSha256: sha('node'), runner: { cpuModel: 'fixture CPU', imageOS: 'ubuntu', imageVersion: 'fixture' } });
  for (const arm of ['baseline', 'candidate']) {
    const files = { 'package.json': '{}', 'safe_cex_wasm.js': '// fixture', 'safe_cex_wasm_bg.wasm': 'fixture' };
    for (const [name, bytes] of Object.entries(files)) write(`${arm}/node/${name}`, bytes);
    write(`${arm}/node/build.json`, { sourceHead: refs[arm], runId: '10',
      hashes: Object.fromEntries(Object.entries(files).map(([name, bytes]) => [name, sha(bytes)])) });
  }
  const receipts = BITGET_COHORT_ORDER.map((arm, index) => {
    const prefix = `${String(index + 1).padStart(2, '0')}-${arm}`;
    write(`${prefix}/before.json`, { number: index + 1, arm });
    const artifacts = Array.from({ length: 20 }, (_, i) => {
      const name = `slot-${i}.json`; write(`${prefix}/${name}`, { impl: i % 2 ? 'rust' : 'ts', metrics: { steadyCpuMs: 1 } });
      return { path: `/producer/out/${name}`, impl: i % 2 ? 'rust' : 'ts', mode: i < 2 ? 'correctness' : 'performance', run: i };
    });
    write(`${prefix}/gate.json`, { artifacts });
    return { number: index + 1, arm, finishedAt: '2026-10-26T01:00:00Z' };
  });
  write('comparison.json', { cohorts: receipts });
  return { root, write, json: name => JSON.parse(readFileSync(resolve(root, name), 'utf8')) };
}

test('reader derives seals from independent package bytes and retains all 160 original slots', t => {
  const { root } = fixture(t), result = readBitgetPairedEvidence(root);
  assert.equal(result.cohorts.length, 8);
  assert.ok(result.cohorts.every(row => row.evidence.raw.length === 20));
  assert.equal(result.archives.baseline.files['safe_cex_wasm_bg.wasm'], sha('fixture'));
});

test('identity is computed from every shipped node/web file, never a supplied hash claim', t => {
  const x = fixture(t);
  for (const arm of ['baseline', 'candidate']) for (const target of ['node', 'web']) {
    const names = ['safe_cex_wasm.js', 'safe_cex_wasm_bg.wasm', 'safe_cex_wasm.d.ts', 'safe_cex_wasm_bg.wasm.d.ts',
      ...(target === 'node' ? ['package.json'] : [])];
    for (const name of names) x.write(`${arm}/shipped/${target}/${name}`,
      target === 'node' && !name.endsWith('.d.ts') ? readFileSync(resolve(x.root, arm, 'node', name)) : `fixture ${target}/${name}`);
  }
  assert.equal(identicalBitgetShipping(readBitgetPairedEvidence(x.root).archives), true);
  const plan = x.json('plan.json'); plan.shippingIdentical = true; plan.shippingSha256 = sha('forged'); x.write('plan.json', plan);
  x.write('candidate/shipped/web/safe_cex_wasm.js', 'one byte different');
  const changed = readBitgetPairedEvidence(x.root);
  assert.equal(changed.archives.candidate.shipped.web['safe_cex_wasm.js'], sha('one byte different'));
  assert.equal(identicalBitgetShipping(changed.archives), false);
  rmSync(resolve(x.root, 'candidate/shipped/web/safe_cex_wasm.d.ts'));
  assert.throws(() => readBitgetPairedEvidence(x.root), /shipped.*inventory/);
});

test('duplicate raw files, symlink escape, truncated inventories and protocol edits fail closed', async t => {
  for (const mutation of ['duplicate', 'symlink', 'truncated', 'protocol', 'tolerance', 'anchorOverride', 'ambiguous-json', 'invalid-utf8']) await t.test(mutation, t => {
    const x = fixture(t);
    if (mutation === 'duplicate') {
      const report = x.json('01-baseline/gate.json'); report.artifacts[1].path = report.artifacts[0].path;
      x.write('01-baseline/gate.json', report);
    } else if (mutation === 'symlink') {
      rmSync(resolve(x.root, '01-baseline/slot-0.json'));
      symlinkSync(resolve(x.root, 'plan.json'), resolve(x.root, '01-baseline/slot-0.json'));
    } else if (mutation === 'truncated') {
      const report = x.json('01-baseline/gate.json'); report.artifacts.pop(); x.write('01-baseline/gate.json', report);
    } else if (mutation === 'protocol') {
      const plan = x.json('plan.json'); plan.protocolSha256 = sha('different'); x.write('plan.json', plan);
    } else if (mutation === 'tolerance') {
      const plan = x.json('plan.json'); plan.T = 2; x.write('plan.json', plan);
    } else if (mutation === 'anchorOverride') {
      const plan = x.json('plan.json'); plan.A0 = 'b'.repeat(40); x.write('plan.json', plan);
    } else if (mutation === 'ambiguous-json') x.write('01-baseline/slot-0.json', '{"metrics":{},"metrics":{}}');
    else x.write('01-baseline/slot-0.json', Buffer.from([123, 34, 120, 34, 58, 34, 255, 34, 125]));
    assert.throws(() => readBitgetPairedEvidence(x.root));
  });
});
