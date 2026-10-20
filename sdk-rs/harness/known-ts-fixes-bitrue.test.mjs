import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fixBitrue, BITRUE_TS_FIXES } from './known-ts-fixes-bitrue.mjs';

// Exercise the actual generator without exporting its implementation as an oracle.
const directory = mkdtempSync(join(tmpdir(), 'bitrue-execution-oracle-'));
let records;
try {
  const output = join(directory, 'private.jsonl');
  execFileSync('corepack', ['yarn', '-s', 'rust:private-tape-bitrue', '--out', output], { stdio: 'pipe' });
  records = readFileSync(output, 'utf8').trim().split('\n').map(JSON.parse);
} finally {
  rmSync(directory, { recursive: true, force: true });
}
const id = '2334151155877155281';
const fill = (amount, price) => ({ id, symbol: 'E-BTC-USDT', side: 'buy', amount, price, executionId: null, fee: null, timestamp: null });
const legacy = () => [fill(0.001, 100), fill(0.002, 130), fill(0.002, 170)];
// Four observed one-contract executions. z=4 was never reported; z=2 is late.
const native = () => [fill(0.001, 100), fill(0.001, 130), fill(0.001, 120), fill(0.001, 170)];
const output = () => ({ account: { fills: legacy(), balance: { total: 71 }, orders: [{ id: 'untouched', amount: 19 }],
  positions: [{ contracts: 8 }], settings: [{ leverage: 21 }], hedged: false },
  commands: [{ error: 'native rejection' }], requests: ['unserved request'], writes: ['unexpected write'],
  unserved: 1, missingWrites: 2, invalidSignatures: 3, unusedWrites: 4, protocolErrors: ['invalid native quantity'] });
const indices = (tape) => tape.flatMap((r, index) => r.type === 'ws' && r.text.includes('"x":"TRADE"') ? [index] : []);
const patch = (tape, index, from, to) => {
  const record = tape[indices(tape)[index]];
  assert.ok(record.text.includes(from), `missing negative-control anchor ${from}`);
  record.text = record.text.replace(from, to);
};

test('actual private tape binds four exact native executions and the known three-fill TS defect', () => {
  const ts = output();
  const before = structuredClone(ts);
  const tapeBefore = structuredClone(records);
  fixBitrue(ts, records);
  assert.deepEqual(ts.account.fills, native());
  const expected = structuredClone(before);
  expected.account.fills = native();
  assert.deepEqual(ts, expected, 'only TS account.fills may change');
  assert.deepEqual(records, tapeBefore, 'the input tape must not change');
  fixBitrue(ts, records);
  assert.deepEqual(ts, expected, 'an already corrected exact oracle is accepted');
  assert.equal(fixBitrue.length, 2, 'the oracle has no Rust-output argument');
  assert.equal(BITRUE_TS_FIXES.length, 1);
});

test('native output and independent fields remain different when corrupted', () => {
  const ts = output();
  const rust = structuredClone(ts);
  rust.account.fills = native();
  rust.account.fills[0].price = 999;
  rust.account.fills[1].amount = 0.002;
  const before = structuredClone(rust);
  Object.freeze(rust);
  fixBitrue(ts, records);
  assert.deepEqual(rust, before);
  assert.notDeepEqual(ts.account.fills, rust.account.fills, 'wrong native quantity/price remains a mismatch');
  assert.equal(ts.account.balance.total, 71);
  assert.equal(ts.account.orders[0].amount, 19);
  assert.deepEqual(ts.commands, [{ error: 'native rejection' }]);
  assert.equal(ts.invalidSignatures, 3);
  assert.equal(ts.unserved, 1);
  assert.deepEqual(ts.protocolErrors, ['invalid native quantity']);
});

const tapeDefects = {
  'missing duplicate': (tape) => tape.splice(indices(tape)[2], 1),
  'extra duplicate': (tape) => tape.push(structuredClone(tape[indices(tape)[2]])),
  'different gap': (tape) => patch(tape, 1, '"z":"3"', '"z":"4"'),
  'changed native amount': (tape) => patch(tape, 1, '"l":"1"', '"l":"2"'),
  'changed native price': (tape) => patch(tape, 1, '"L":"130"', '"L":"131"'),
  'changed order identity': (tape) => patch(tape, 1, id, '2334151155877155282'),
  'changed side': (tape) => patch(tape, 1, '"S":"BUY"', '"S":"SELL"'),
  'changed native symbol': (tape) => patch(tape, 1, 'E_BTCUSDT', 'E_ETHUSDT'),
  'changed terminal status': (tape) => patch(tape, 4, '"X":"FILLED"', '"X":"PART_FILLED"'),
  'changed row fields': (tape) => patch(tape, 1, '"R":false', '"R":false,"unexpected":true'),
  'changed cumulative placeholder': (tape) => patch(tape, 1, '"ap":"130"', '"ap":"120"'),
  'late interval delivered earlier': (tape) => {
    const rows = indices(tape); [tape[rows[1]], tape[rows[3]]] = [tape[rows[3]], tape[rows[1]]];
  },
  'changed event time': (tape) => { tape[indices(tape)[1]].t++; },
  'changed scheduling': (tape) => { tape[indices(tape)[1]].availableAfter = 99999; },
  'unknown execution': (tape) => {
    const extra = structuredClone(tape[indices(tape)[1]]); extra.text = extra.text.replace(id, '999'); tape.push(extra);
  },
  'missing all execution rows': (tape) => { for (const index of indices(tape).reverse()) tape.splice(index, 1); },
  'changed contract multiplier': (tape) => {
    const r = tape.find((row) => row.type === 'http' && row.url.endsWith('/contracts'));
    const body = JSON.parse(r.body); body[0].multiplier = 0.01; r.body = JSON.stringify(body);
  },
  'changed startup execution prefix': (tape) => {
    const r = tape.find((row) => row.type === 'http' && row.url.includes('/openOrders?contractName=E-BTC-USDT'));
    const body = JSON.parse(r.body); body.data[0].orderId = id; body.data[0].executedQty = 0.001; r.body = JSON.stringify(body);
  },
};
for (const [label, corrupt] of Object.entries(tapeDefects)) {
  test(`correction rejects ${label} before changing either accepted TS fill list`, () => {
    for (const fills of [legacy(), native()]) {
      const tape = structuredClone(records); corrupt(tape);
      const ts = output(); ts.account.fills = fills;
      const before = structuredClone(ts);
      assert.throws(() => fixBitrue(ts, tape), /Bitrue/);
      assert.deepEqual(ts, before);
    }
  });
}

for (const [label, corrupt] of Object.entries({
  amount: (fills) => { fills[1].amount = 0.0015; },
  price: (fills) => { fills[1].price++; },
  identity: (fills) => { fills[1].id = 'incorrect'; },
  symbol: (fills) => { fills[1].symbol = 'E-ETH-USDT'; },
  side: (fills) => { fills[1].side = 'sell'; },
  omission: (fills) => { fills.pop(); },
  duplicate: (fills) => { fills.push(structuredClone(fills[0])); },
  reordering: (fills) => { fills.reverse(); },
  unknownField: (fills) => { fills[0].unexpected = true; },
})) {
  test(`unexpected TS ${label} is not normalized`, () => {
    const ts = output(); corrupt(ts.account.fills);
    const before = structuredClone(ts);
    assert.throws(() => fixBitrue(ts, records), /unexpected TS Bitrue cumulative-delta fills/);
    assert.deepEqual(ts, before);
  });
}

test('public and execution-free smoke tapes need no correction', () => {
  const ts = { tickers: [{ last: 123 }] };
  fixBitrue(ts, [{ type: 'meta', private: false }]);
  assert.deepEqual(ts, { tickers: [{ last: 123 }] });
  const privateTs = { account: { fills: [] } };
  fixBitrue(privateTs, [{ type: 'meta', private: true }]);
  assert.deepEqual(privateTs, { account: { fills: [] } });
});
