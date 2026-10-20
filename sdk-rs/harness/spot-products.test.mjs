import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { venueOf } from './venues.mjs';
import { createKrakenSpotRequestAudit, krakenSpotSignature, krakenSpotRequestBody, krakenSpotCommandResult, validateKrakenSpotSocketMessage, validateKrakenSpotSocketFrame } from './krakenspot.mjs';
import { validateHyperliquidSpotRequest, HYPERLIQUID_SPOT_REPLAY } from './hyperliquidspot.mjs';
import { buildHyperliquidSpotTape } from './private-tape-hyperliquidspot.mjs';
import { buildKrakenSpotTape } from './private-tape-krakenspot.mjs';
const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));

test('spot aliases use their existing crate feature without selecting the perpetual TS reference', () => {
  for (const name of ['hyperliquidspot', 'krakenspot']) {
    const venue = venueOf(name);
    assert.equal(venue.rust, name);
    assert.equal(venue.ts.name, name);
    assert(venue.ts.module.endsWith(`/${name}.exchange.ts`));
  }
});
test('documented spot tapes retain native identities, private classification and all trading writes', () => {
  const hyper = buildHyperliquidSpotTape();
  assert.equal(hyper[0].private, true);
  const action = JSON.parse(hyper.find((r) => r.type === 'http' && r.url.endsWith('/exchange')).requestBody).action;
  assert.equal(action.orders[0].a, 10001);
  assert.equal(action.orders[0].r, false);
  assert(hyper.some((r) => r.type === 'ws' && JSON.parse(r.text).channel === 'userFills'));
  const kraken = buildKrakenSpotTape();
  const writes = kraken.filter((r) => r.type === 'http' && /\/(AddOrder|AmendOrder|CancelOrder)$/.test(r.url));
  assert.equal(writes.length, 7);
  assert(writes.every((r) => r.afterCommand > 0));
  assert.deepEqual(writes.filter((r) => r.url.endsWith('/AddOrder')).map((r) => new URLSearchParams(r.requestBody).get('ordertype')),
    ['limit', 'market', 'stop-loss', 'take-profit', 'trailing-stop']);
  const amend = new URLSearchParams(writes.find((r) => r.url.endsWith('/AmendOrder')).requestBody);
  assert.equal(amend.get('txid'), 'O-100');
  assert.equal(amend.get('post_only'), 'true');
  assert(kraken.some((r) => r.type === 'close'));
  assert(kraken.some((r) => r.type === 'ws' && r.socket === 1));
});
test('Hyperliquid Spot shared-URL routing follows actual subscriptions and the current spot-state alias', () => {
  const venue = venueOf('hyperliquidspot');
  const publicSocket = {}, privateSocket = {};
  const subscribe = (socket, type) => venue.onSocketSend(socket, JSON.stringify({ method: 'subscribe', subscription: { type } }));
  subscribe(publicSocket, 'allMids');
  subscribe(privateSocket, 'spotClearinghouseState');
  subscribe(privateSocket, 'userFills');
  const matches = (socket, channel) => venue.socketMatches(socket, JSON.stringify({ channel, data: {} }));
  assert.equal(matches(publicSocket, 'allMids'), true);
  assert.equal(matches(privateSocket, 'allMids'), false);
  assert.equal(matches(privateSocket, 'userFills'), true);
  assert.equal(matches(publicSocket, 'userFills'), false);
  assert.equal(matches(privateSocket, 'spotState'), true);
  assert.equal(matches(publicSocket, 'spotState'), false);
  assert.equal(matches(privateSocket, 'orderUpdates'), false, 'an unrequested channel cannot satisfy routing');
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
test('Kraken Spot caller-ID wording normalization cannot hide unrelated failures or missing rejection', () => {
  const command = { op: 'placeOrders', orders: [{ clientOrderId: 'caller' }] };
  assert.deepEqual(krakenSpotCommandResult({ error: 'krakenspot cannot honor a caller clientOrderId' }, command), { error: 'caller client order ID unsupported' });
  for (const result of [{ ok: [] }, { error: 'HTTP 500' }]) assert.deepEqual(krakenSpotCommandResult(result, command), result);
  const missingCaller = { op: 'placeOrders', orders: [{}] }, result = { error: 'caller client IDs' };
  assert.deepEqual(krakenSpotCommandResult(result, missingCaller), result);
});
test('Kraken Spot void cancellation labels preserve failed and non-void command results', () => {
  assert.deepEqual(krakenSpotCommandResult({ ok: null }, { op: 'cancelOrders' }), { ok: [] });
  for (const result of [{ error: 'invalid ACK' }, {}, { ok: ['unexpected native ID'] }, { ok: null, error: 'failed' }])
    assert.deepEqual(krakenSpotCommandResult(result, { op: 'cancelOrders' }), result);
  assert.deepEqual(krakenSpotCommandResult({ ok: null }, { op: 'placeOrders' }), { ok: null });
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
  assert.throws(() => validateKrakenSpotSocketMessage(second, subscribe, now, state), /current REST token/);
  validateKrakenSpotSocketMessage(second, subscribe.replace('replay-spot-token', 'replay-spot-token-2'), now, state);
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
