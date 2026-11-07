import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { coinbasePrivateTape } from './private-tape-coinbase.mjs';
import { validateAcceptedDifferences, validateKnownFailures } from './known-failure-policy.mjs';
import { RUST_ONLY_FAILURE } from './rust-only-oracle.mjs';

const known = JSON.parse(readFileSync(new URL('./known-gate-failures.json', import.meta.url)));
const accepted = JSON.parse(readFileSync(new URL('./accepted-differences.json', import.meta.url)));
const diagnosticSets = {
  cfm: 'fee3474fe2f37db23083768f72b4e4ff4ba02a9f34d1fe6f9e4722ae114f8549',
  spot: '44031bc9fe03b0d09e4849ba9931341dd301b58990ac29f4db4f1a839eba99ed',
};
const hash = (text) => createHash('sha256').update(text).digest('hex');

for (const venue of ['coinbase', 'coinbasespot'])
  for (const profile of venue === 'coinbase' ? ['intx', 'cfm', 'spot'] : ['spot'])
    for (const algorithm of ['ES256', 'EdDSA']) {
      const tape = `sdk-rs/harness/tapes/${venue}-${profile}-${algorithm}-private.jsonl`;
      const { records } = coinbasePrivateTape({ venue, profile, algorithm });
      const tapeSha256 = hash(records.map((record) => JSON.stringify(record)).join('\n') + '\n');

      test(`${venue}/${profile}/${algorithm} binds only the unchanged historical diagnostic set to its actual tape`, () => {
        const entry = known.failures.find((entry) => entry.venue === venue && entry.tape === tape);
        assert.equal(entry?.tapeSha256, tapeSha256);
        const keys = entry.diagnostics.flatMap((row) => row.keys ?? [row.key]).sort();
        // INTX replays Rust alone: its only declaration is the retired TS oracle.
        if (profile === 'intx') assert.deepEqual(keys, [RUST_ONLY_FAILURE]);
        else assert.equal(hash(keys.join('\n') + '\n'), diagnosticSets[profile]);
        assert.deepEqual(validateKnownFailures(known), []);
      });

      test(`${venue}/${profile}/${algorithm} accepts exactly the intended spot cash root values`, () => {
        const entry = accepted.differences.find((entry) => entry.venue === venue && entry.tape === tape);
        if (profile !== 'spot') {
          assert.equal(entry, undefined);
          return;
        }
        assert.equal(entry?.tapeSha256, tapeSha256);
        const breakdown = records.find((row) => row.type === 'http' && row.url.endsWith('/portfolios/replay-portfolio'));
        const cash = JSON.parse(breakdown.body).breakdown.spot_positions.find((row) => row.asset === 'USD');
        assert.equal(cash.account_type, 'ACCOUNT_TYPE_FIAT');
        assert.equal(cash.total_balance_crypto, 1000);
        assert.equal(cash.available_to_trade_crypto, 900);
        const expected = [];
        for (const [key, value] of [['total', 1000], ['free', 900], ['used', 100]]) {
          expected.push(`account balance.${key}: ts=0 rust=${value}`);
          for (let checkpoint = 0; checkpoint < 19; checkpoint++)
            expected.push(`checkpoint ${checkpoint} account.balance.${key}: ts=0 rust=${value}`);
        }
        assert.deepEqual(entry.diagnostics.flatMap((row) => row.failures).sort(), expected.sort());
        assert.ok(entry.diagnostics.every((row) => row.category === 'rust-better'));
        assert.deepEqual(validateAcceptedDifferences(accepted, known), []);
      });
    }
