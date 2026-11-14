import assert from 'node:assert/strict';
import { test } from 'node:test';
import { RUST_ONLY, rustOnlyDifferences, rustOnlyOracle } from './rust-only-oracle.mjs';

const account = { hedged: false, balance: { total: 1, free: 1, used: 0, upnl: 0 }, orders: [], positions: [],
  flatPositionPnl: [], settings: [], fills: [] };
const tape = (meta) => [{ type: 'meta', private: true, ...meta }, { type: 'config', private: true }];

test('only a private tape with a complete expected state can retire its TS oracle', () => {
  assert.equal(rustOnlyOracle(tape({}), true), false);
  assert.equal(rustOnlyOracle(tape({ oracle: RUST_ONLY, expected: { account, commands: [] } }), true), true);
  assert.throws(() => rustOnlyOracle(tape({ oracle: 'none' }), true), /unknown replay oracle/);
  assert.throws(() => rustOnlyOracle(tape({ oracle: RUST_ONLY, expected: { account, commands: [] } }), false), /must be private/);
  assert.throws(() => rustOnlyOracle(tape({ oracle: RUST_ONLY }), true), /expected account/);
  const { fills: _fills, ...partial } = account;
  assert.throws(() => rustOnlyOracle(tape({ oracle: RUST_ONLY, expected: { account: partial, commands: [] } }), true), /expected account/);
});

test('Rust is held to the expected state and to every transport integrity check', () => {
  const records = tape({ oracle: RUST_ONLY, expected: { account, commands: [{ ok: true }] } });
  const output = {
    account: { ...account, balance: { ...account.balance, free: 2 } }, commands: [{ ok: false }],
    markets: [], tickers: [], books: {}, unserved: 1, missingWrites: 2, invalidSignatures: 3, unusedWrites: 4,
    protocolErrors: ['socket audit'], checkpoints: [],
  };
  const found = rustOnlyDifferences(output, { tapeRecords: records, checkpointPlan: [], books: '', run: 0, venue: 'coinbase' }).join('\n');
  for (const expected of [
    'expected account balance.free: ts=1 rust=2', 'expected command results',
    'rust: 1 request(s) matched nothing', 'rust: 2 required taped write(s) never sent', 'rust: 3 invalid signatures',
    'rust: 4 taped mutation(s) never sent', 'rust: socket audit', 'rust: empty public seed',
  ]) assert.ok(found.includes(expected), `${expected}\n${found}`);
});
