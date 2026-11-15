import assert from 'node:assert/strict';
import test from 'node:test';
import { cpuAnnotations, writeCpuAnnotations } from './cpu-annotations.mjs';

const samples = (factor = 1) => [1, 3, 2, 4].map(value => ({
  cpuMs: value * factor, steadyCpuMs: value * 0.1 * factor,
  processCpuMs: value * 2 * factor, retainedBytes: 100 * factor,
}));
const report = failures => ({ venue: 'binance', tape: 'public.jsonl.gz', failures, warnings: [],
  samples: { ts: samples(), rust: samples(0.8) } });

test('a successful replay logs exact TS/Rust medians and Rust/TS ratios for all CPU metrics', () => {
  const lines = [];
  writeCpuAnnotations(report([]), undefined, line => lines.push(line));
  const metrics = lines.filter(line => line.startsWith('CPU_METRICS ')).map(line => JSON.parse(line.slice(12)));
  assert.equal(metrics.length, 3, 'successful measurements must not disappear from the job log');
  assert.deepEqual(metrics.map(({ field }) => field), ['cpuMs', 'steadyCpuMs', 'processCpuMs']);
  for (const value of metrics) {
    assert.equal(value.venue, 'binance');
    assert.equal(value.tape, 'public.jsonl.gz');
    assert.equal(value.rustTsRatio, value.rustMs / value.tsMs);
  }
  assert.deepEqual(metrics[0], { venue: 'binance', tape: 'public.jsonl.gz', field: 'cpuMs', tsMs: 2.5, rustMs: 2, rustTsRatio: 0.8 });
  assert.equal(metrics[1].tsMs, 0.25, 'sub-ms values retain precision');
  assert.ok(metrics[1].rustMs > 0.19 && metrics[1].rustMs < 0.21);
  assert.ok(lines.every(line => !line.startsWith('::warning') && !line.startsWith('::error')));
  assert.equal(cpuAnnotations(report([])).summary, '', 'observations do not manufacture warnings');
});

test('failed replay measurements keep their existing CPU error annotation alongside the values', () => {
  const failure = 'performance steady-state CPU: Rust is slower than TS (ts=0.25, rust=0.5, 2.000x)';
  const value = report([failure]);
  value.samples.rust = samples(2);
  const { lines } = cpuAnnotations(value);
  assert.equal(lines.filter(line => line.startsWith('CPU_METRICS ')).length, 3);
  assert.ok(lines.includes(`::error title=CPU binance::public.jsonl.gz: ${failure}`));
});

test('callers without measured samples keep their existing annotation-only behavior', () => {
  assert.deepEqual(cpuAnnotations({ venue: 'binance', tape: 'private.jsonl', failures: [], warnings: [] }), { lines: [], summary: '' });
});
