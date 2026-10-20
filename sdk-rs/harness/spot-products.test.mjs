import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { venueOf } from './venues.mjs';
import { createKrakenSpotRequestAudit, krakenSpotSignature, krakenSpotRequestBody, validateKrakenSpotSocketMessage, validateKrakenSpotSocketFrame } from './krakenspot.mjs';
import { validateHyperliquidSpotRequest, HYPERLIQUID_SPOT_REPLAY } from './hyperliquidspot.mjs';
const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));

test('spot aliases use their existing crate feature without selecting the perpetual TS reference', () => {
  for (const name of ['hyperliquidspot', 'krakenspot']) {
    const venue = venueOf(name);
    assert.equal(venue.rust, name);
    assert.equal(venue.ts.name, name);
    assert(venue.ts.module.endsWith(`/${name}.exchange.ts`));
  }
});
test('Kraken Spot signature matches the official published vector', () => {
  const body = 'nonce=1616492376594&ordertype=limit&pair=XBTUSD&price=37500&type=buy&volume=1.25';
  assert.equal(krakenSpotSignature('/0/private/AddOrder', body, 'kQH5HW/8p1uGOVjbgWA7FunAmGO8lsSUXNsu3eow76sz84Q18fWxnyRzBHCd3pd5nE9qa99HAZtuZuj6F1huXg=='),
    '4/dpxb3iT4tp/ZCVEwSnEsLxx0bqyhLpdfOpc6fn7OR8+UClSV5n9E6aSS8MPtnRfp32bAb0nmbRn6H8ndwLUQ==');
});
const now = 1800000000000;
function request(body = `nonce=${now * 1000}&pair=XXBTZUSD&ordertype=limit&type=buy&price=90&volume=1&cl_ord_id=${'a'.repeat(32)}`, path = '/0/private/AddOrder') {
  return { method: 'POST', url: `https://api.kraken.com${path}`, body, now,
    headers: { 'API-Key': 'replay-key', 'API-Sign': krakenSpotSignature(path, body) } };
}
test('Kraken Spot oracle binds exact nonce, encoded form, native route and generated identity', () => {
  const r = request();
  assert.doesNotThrow(() => createKrakenSpotRequestAudit()(r));
  for (const patch of [{ body: r.body.replace('price=90', 'price=91') }, { url: r.url.replace('AddOrder', 'AmendOrder') }, { headers: { ...r.headers, 'API-Key': 'wrong' } }])
    assert.throws(() => createKrakenSpotRequestAudit()({ ...r, ...patch }));
  const audit = createKrakenSpotRequestAudit();
  audit(r);
  assert.throws(() => audit(r), /nonce/);
  assert.throws(() => createKrakenSpotRequestAudit()(request(r.body + '&userref=1')), /conflicting/);
  assert.throws(() => createKrakenSpotRequestAudit()(request(r.body.replace('cl_ord_id=', 'cl_ord_id=x'))), /client ID/);
  assert.throws(() => createKrakenSpotRequestAudit()(request(r.body + '&volume=2')), /duplicate parameter/);
});
test('Kraken Spot canonical matching preserves all economic amend fields', () => {
  assert.deepEqual(krakenSpotRequestBody({ nonce: '42', cl_ord_id: 'a'.repeat(32), txid: 'native', order_qty: '2', limit_price: '91', post_only: 'true' }),
    { cl_ord_id: '<generated>', txid: 'native', order_qty: '2', limit_price: '91', post_only: 'true' });
});
test('Kraken Spot reconnect cannot reuse prior token issuance or authenticated subscription', () => {
  const state = { tokenRequests: 1 };
  const first = { url: 'wss://ws-auth.kraken.com/v2' };
  const subscribe = JSON.stringify({ method: 'subscribe', params: { channel: 'executions', token: 'replay-spot-token' } });
  const data = JSON.stringify({ channel: 'executions', type: 'update', data: [] });
  assert.throws(() => validateKrakenSpotSocketFrame(first, data), /before/);
  validateKrakenSpotSocketMessage(first, subscribe, now, state);
  validateKrakenSpotSocketFrame(first, data);
  const second = { url: first.url };
  assert.throws(() => validateKrakenSpotSocketMessage(second, subscribe, now, state), /fresh token/);
  state.tokenRequests++;
  validateKrakenSpotSocketMessage(second, subscribe, now, state);
  assert.throws(() => validateKrakenSpotSocketMessage({ url: 'wss://ws.kraken.com/v2' }, subscribe), /current REST token/);
});

test('Hyperliquid Spot independent EIP-712 oracle rejects signed economic mutations and perp indices', async () => {
  const { ethers } = require('ethers'), { encode } = require('@msgpack/msgpack');
  const sign = async (action) => {
    const suffix = Buffer.alloc(9); suffix.writeBigUInt64BE(BigInt(now));
    const connectionId = ethers.keccak256(Buffer.concat([encode(action), suffix]));
    const signature = ethers.Signature.from(await new ethers.Wallet(HYPERLIQUID_SPOT_REPLAY.secret).signTypedData(
      { name: 'Exchange', version: '1', chainId: 1337, verifyingContract: ethers.ZeroAddress },
      { Agent: [{ name: 'source', type: 'string' }, { name: 'connectionId', type: 'bytes32' }] }, { source: 'a', connectionId })).toJSON();
    return { action, nonce: now, signature };
  };
  const action = { type: 'order', orders: [{ a: 10001, b: true, p: '90', s: '1', r: false, t: { limit: { tif: 'Gtc' } } }], grouping: 'na' };
  const frame = await sign(action);
  const verify = (value) => validateHyperliquidSpotRequest({ method: 'POST', url: 'https://api.hyperliquid.xyz/exchange', body: JSON.stringify(value), now });
  assert.doesNotThrow(() => verify(frame));
  assert.throws(() => verify({ ...frame, action: { ...action, orders: [{ ...action.orders[0], p: '91' }] } }));
  const perp = await sign({ ...action, orders: [{ ...action.orders[0], a: 1 }] });
  assert.throws(() => verify(perp), /spot indices/);
  assert.throws(() => verify({ ...frame, nonce: now - 60001 }), /nonce/);
});
