import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildParityReport, entityOf, renderMarkdown } from './parity-report.mjs';

const metric = () => ({
  baselineKind: 'idle-loaded', measuredMode: 'correctness', cpuMs: 10, processCpuMs: 10, steadyCpuMs: 10,
  idleLoadCpuMs: 1, idleLoadProcessCpuMs: 1, idleLoadWallMs: 1, runtimeCpuMs: 9, runtimeProcessCpuMs: 9, runtimeWallMs: 9, wallMs: 10,
  heapBeforeBytes: 100, heapUsedBytes: 110, heapRetainedBytes: 10, buffersBefore: 0, buffersAfter: 0, bufferRetainedBytes: 0,
  wasmBeforeBytes: 0, wasmBytes: 0, wasmRetainedBytes: 0, retainedBytes: 10,
});
const report = (venue, failures) => ({
  schema: 1, venue, tape: `sdk-rs/harness/tapes/${venue}-private.jsonl`, tapeSha256: 'a'.repeat(64), private: true,
  status: failures.length ? 'FAIL' : 'PASS', failures, correctnessRuns: 1, runs: 0, samples: { ts: [metric()], rust: [metric()] },
});

test('diagnostic keys map to the normalized output they compare', () => {
  for (const [key, entity] of [
    ['checkpoint 3 account.orders[0].clientOrderId', 'orders'],
    ['account orders.length', 'orders'],
    ['account positions[1].marginSize', 'positions'],
    ['checkpoint 0 account.balance.total', 'balance'],
    ['account fills[0].price', 'fills'],
    ['account flatPositionPnl.length', 'flatPositionPnl'],
    ['checkpoint 1 account.hedged', 'account'],
    ['command results[2].error.kind', 'commands'],
    ['checkpoint 0 books.BTCUSDT.asks[0][0]', 'books'],
    ['book BTCUSDT.bids[0]', 'books'],
    ['market BTCUSDT.precision.amount', 'markets'],
    ['checkpoint 0 tickers[3].percentage', 'tickers'],
    ['HTTP write sequence run 1', 'transport'],
    ['performance cpuMs', 'performance'],
    ['fixer modified Rust candidate output', 'other'],
  ]) assert.equal(entityOf(key), entity, key);
});

test('the report counts accepted, known and new diagnostics per venue and entity', () => {
  const known = { schema: 1, failures: [{ venue: 'alpha', tapeSha256: 'a'.repeat(64), diagnostics: [
    { key: 'account orders[0].clientOrderId', divergence: 'Debt.' },
  ] }] };
  const accepted = { schema: 1, differences: [{ venue: 'alpha', tapeSha256: 'a'.repeat(64), tape: 'sdk-rs/harness/tapes/alpha-private.jsonl',
    diagnostics: [{ keys: ['checkpoint 2 account.positions[0].updatedAt'], category: 'rust-better', rationale: 'Fresher epoch.', evidence: 'frame 12' }] }] };
  const result = buildParityReport([
    report('beta', []),
    report('alpha', [
      'account orders[0].clientOrderId: ts="c" rust=null',
      'checkpoint 2 account.positions[0].updatedAt: ts=1 rust=2',
      'command results[0].error.kind: ts="unknown" rust="rateLimit"',
    ]),
  ], known, accepted);
  assert.deepEqual(result.totals, { accepted: 1, known: 1, new: 1 });
  const [alpha, beta] = result.cases;
  assert.equal(alpha.venue, 'alpha');
  assert.equal(alpha.status, 'NEW DIVERGENCE OR INVALID');
  assert.equal(alpha.entities.orders.known.count, 1);
  assert.equal(alpha.entities.positions.accepted.count, 1);
  assert.equal(alpha.entities.commands.new.count, 1);
  assert.equal(beta.status, 'PASS');
  const markdown = renderMarkdown(result);
  assert.match(markdown, /\| alpha \| alpha-private\.jsonl \| NEW DIVERGENCE OR INVALID \| commands \| 0 \| 0 \| 1 \|/);
  assert.match(markdown, /\| beta \| beta-private\.jsonl \| PASS \| - \| 0 \| 0 \| 0 \|/);
  assert.match(markdown, /alpha: unexpected command results\[0\]\.error\.kind/);
});

test('a gate with only accepted differences reports as accepted, not as debt', () => {
  const accepted = { schema: 1, differences: [{ venue: 'alpha', tapeSha256: 'a'.repeat(64), tape: 'sdk-rs/harness/tapes/alpha-private.jsonl',
    diagnostics: [{ keys: ['account fills[0].timestamp'], category: 'ts-unsupported', rationale: 'TS drops the native fill time.', evidence: 'frame 4' }] }] };
  const result = buildParityReport([report('alpha', ['account fills[0].timestamp: ts=null rust=5'])], { schema: 1, failures: [] }, accepted);
  assert.equal(result.cases[0].status, 'ACCEPTED DIFFERENCES');
});
