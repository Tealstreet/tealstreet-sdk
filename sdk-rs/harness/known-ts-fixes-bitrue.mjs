// Fixture-specific correction for the documented Bitrue futures l/L contract:
// https://www.bitrue.com/api_docs_includes_file/futures/index.html
// l is this execution's contracts and L prices only those contracts. The legacy
// TS cumulative delta assigns missing executions to L and loses a late interval.
import assert from 'node:assert/strict';
import { isDeepStrictEqual } from 'node:util';
import { parseRequestJson } from './request-json.mjs';

export const BITRUE_TS_FIXES = [
  'Private fixture fills use each distinct native l/L execution; cumulative gaps are not repriced at L.',
];

// Deliberately a closed fixture table, not a second implementation of Rust's
// interval tracker. z=4 is unobserved: no quantity or price is invented for it.
const observations = [[1, 100, 'PART_FILLED'], [3, 130, 'PART_FILLED'],
  [3, 130, 'PART_FILLED'], [2, 120, 'PART_FILLED'], [5, 170, 'FILLED'], [5, 170, 'FILLED']];
const orderId = '2334151155877155281';
const privateUrl = 'wss://fapiws.bitrue.com/stream?listenKey=replay-listen-key';
// Replay fill rows carry execution metadata; TS has none for these rows.
const fill = (amount, price) => ({ id: orderId, symbol: 'E-BTC-USDT', side: 'buy', amount, price,
  executionId: null, fee: null, timestamp: null });

/** Only TS fills can change; the native output is never an input. */
export function fixBitrue(ts, records) {
  const trades = records.filter((r) => r.type === 'ws').flatMap((record) => {
    const event = parseRequestJson(record.text);
    return event.o?.x === 'TRADE' || event.x === 'TRADE' ? [{ record, event }] : [];
  });
  if (trades.length === 0) {
    assert.deepEqual(ts.account?.fills ?? [], [], 'unexpected TS Bitrue fills without native executions');
    return;
  }
  assert.deepEqual(records.filter((r) => r.type === 'meta'),
    [{ t: 0, type: 'meta', epoch: 1700000000000, private: true }], 'unknown Bitrue execution fixture');
  const catalogs = records.filter((r) => r.type === 'http' && r.method === 'GET'
    && r.url === 'https://fapi.bitrue.com/fapi/v1/contracts');
  assert.equal(catalogs.length, 1, 'missing unique Bitrue contract-size proof');
  assert.equal(catalogs[0].status, 200, 'failed Bitrue contract-size proof');
  const btc = parseRequestJson(catalogs[0].body).filter((r) => r.symbol === 'E-BTC-USDT');
  assert.equal(btc.length, 1, 'missing unique Bitrue BTC contract');
  assert.equal(btc[0].multiplier, 0.001, 'changed Bitrue native contract size');
  assert.equal(trades.length, observations.length, 'changed Bitrue native execution sequence');
  for (let index = 0; index < observations.length; index++) {
    const { record, event } = trades[index];
    const [total, price, status] = observations[index];
    assert.deepEqual(Object.keys(record).sort(), ['t', 'text', 'type', 'url'], 'changed Bitrue execution scheduling');
    assert.equal(record.url, privateUrl, 'changed Bitrue private execution route');
    assert.equal(record.t, 600 + index * 100, 'changed Bitrue execution delivery order');
    assert.equal(record.effectiveAt ?? record.t, record.t, 'changed Bitrue execution delivery time');
    // ap is the generator's existing synthetic placeholder. It is checked to
    // bind this exact fixture, but never supplies a price or a missing execution.
    assert.deepEqual(event, { E: 1700000000000 + record.t, e: 'ORDER_TRADE_UPDATE', o: {
      s: 'E_BTCUSDT', i: BigInt(orderId), c: '', S: 'BUY', o: 'LIMIT', x: 'TRADE', X: status,
      q: '0.005', z: String(total), l: '1', p: '90', ap: String(price), L: String(price), R: false,
    } }, `changed Bitrue native execution row ${index}`);
  }
  // A startup REST prefix for this ID would change which executions are live.
  for (const record of records) {
    if (record.type !== 'http' || record.method !== 'GET'
      || !record.url.startsWith('https://fapi.bitrue.com/fapi/v2/openOrders?')) continue;
    const rows = parseRequestJson(record.body).data;
    assert.ok(Array.isArray(rows), 'invalid Bitrue order census in execution fixture');
    assert.ok(rows.every((row) => String(row.orderId) !== orderId), 'unexpected Bitrue REST execution prefix');
  }
  const legacy = [fill(0.001, 100), fill(0.002, 130), fill(0.002, 170)];
  const expected = [fill(0.001, 100), fill(0.001, 130), fill(0.001, 120), fill(0.001, 170)];
  assert.ok(ts.account, 'missing TS Bitrue private account');
  if (isDeepStrictEqual(ts.account.fills, expected)) return;
  assert.deepEqual(ts.account.fills, legacy, 'unexpected TS Bitrue cumulative-delta fills');
  ts.account.fills = expected;
}
