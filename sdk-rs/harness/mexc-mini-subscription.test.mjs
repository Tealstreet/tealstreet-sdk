import assert from 'node:assert/strict';
import test from 'node:test';
import { withMexcMiniTickerSubscription } from './mexc-mini-subscription.mjs';
import { subscriptionKeys } from './requests.mjs';

const url = 'wss://contract.mexc.com/edge';
const detailed = (symbol) => subscriptionKeys(url, { method: 'sub.ticker', param: { symbol } })[0];
const mini = subscriptionKeys(url, { method: 'sub.tickers', param: {}, gzip: false })[0];

test('MEXC expected mini-feed addition preserves every detailed subscription and is idempotent', () => {
  const original = [detailed('BTC_USDT'), detailed('ETH_USDT')].sort();
  const expected = withMexcMiniTickerSubscription(original);
  assert.deepEqual(expected, [...original, mini].sort());
  assert.deepEqual(withMexcMiniTickerSubscription(expected), expected);
  assert.equal(original.length, 2);
  // A missing detailed feed is still missing: this correction cannot mask it.
  assert.notDeepEqual(withMexcMiniTickerSubscription([original[0]]), expected);
});

test('MEXC mini correction leaves private-only and unrelated feed sets unchanged', () => {
  for (const feeds of [[], ['WS wss://other.example/edge {"method":"sub.ticker"}'],
    subscriptionKeys(url, { method: 'login', param: { apiKey: 'replay-key' } })]) {
    assert.deepEqual(withMexcMiniTickerSubscription(feeds), [...feeds].sort());
  }
});
