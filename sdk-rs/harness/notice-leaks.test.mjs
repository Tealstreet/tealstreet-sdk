import assert from 'node:assert/strict';
import test from 'node:test';

import { credentialValues, findNoticeLeaks } from './notice-leaks.mjs';

test('credential values come from credential-named fields, nested ones included', () => {
  const values = credentialValues({
    key: 'replay-key',
    secret: 'synthetic-fixture-marker',
    passphrase: 'replay-passphrase',
    apiKeyIndex: '3',
    applicationId: '0x0000000000000000000000000000000000000001',
    extra: { omniKeySeed: 'seed-words-for-replay' },
    publicDataOnly: false,
  });
  assert.deepEqual(values.sort(), ['replay-key', 'replay-passphrase', 'seed-words-for-replay', 'synthetic-fixture-marker']);
});

test('a notice quoting a credential or a signed query fails; ordinary rejections pass', () => {
  const credentials = ['replay-key', 'replay-secret'];
  const leaks = findNoticeLeaks([
    'Order rejected: insufficient margin',
    'Bybit orders: 10003 API key is invalid',
    'request failed: {"apiKey":"replay-key","code":10003}',
    'GET https://api.example.test/v5/order?symbol=BTCUSDT&signature=abc123 failed',
    { message: 'nested replay-secret' },
    undefined,
  ], credentials);
  assert.equal(leaks.length, 3);
  assert.match(leaks[0], /^credential in notice: .*<credential>/);
  assert.doesNotMatch(leaks[0], /replay-key/);
  assert.match(leaks[1], /^signed query in notice/);
  assert.match(leaks[2], /^credential in notice/);
});
