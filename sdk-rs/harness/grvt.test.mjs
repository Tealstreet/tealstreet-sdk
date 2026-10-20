import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Wallet, Signature } from 'ethers';
import { venueOf } from './venues.mjs';
import { requestKey } from './requests.mjs';
import { Tape } from './tape.mjs';
const venue = venueOf('grvt');
const REST = 'https://trades.grvt.io';
const PRIVATE = 'wss://trades.grvt.io/ws';
const wallet = new Wallet('0x' + '01'.repeat(32));
const headers = { 'X-Ts-Grvt-Session': 'fixture-session-1', 'X-Grvt-Account-Id': 'fixture-account', Cookie: 'rm=true; gravity=fixture-session-1' };
const types = {
  OrderWithBuilderFee: [
    { name: 'subAccountID', type: 'uint64' }, { name: 'isMarket', type: 'bool' },
    { name: 'timeInForce', type: 'uint8' }, { name: 'postOnly', type: 'bool' },
    { name: 'reduceOnly', type: 'bool' }, { name: 'legs', type: 'OrderLeg[]' },
    { name: 'builder', type: 'address' }, { name: 'builderFee', type: 'uint32' },
    { name: 'nonce', type: 'uint32' }, { name: 'expiration', type: 'int64' },
  ],
  OrderLeg: [
    { name: 'assetID', type: 'uint256' }, { name: 'contractSize', type: 'uint64' },
    { name: 'limitPrice', type: 'uint64' }, { name: 'isBuyingContract', type: 'bool' },
  ],
};
let client = 10000;
async function signed() {
  const now = BigInt(Date.now()) * 1000000n;
  const message = { subAccountID: '777', isMarket: false, timeInForce: 1, postOnly: false, reduceOnly: false,
    legs: [{ assetID: '0x030501', contractSize: '4000000', limitPrice: '90000000000', isBuyingContract: true }],
    builder: '0xee8f3559f298fffa79b8253437969d15b5dccff3', builderFee: 300, nonce: 123,
    expiration: String(now + 3600000000000n) };
  const signature = Signature.from(await wallet.signTypedData({ name: 'GRVT Exchange', version: '0', chainId: 325 }, types, message));
  return { o: { sa: '777', im: false, ti: 'GOOD_TILL_TIME', po: false, ro: false,
    l: [{ i: 'BTC_USDT_Perp', s: '0.004', lp: '90', ib: true }],
    b: message.builder, bf: '0.03', m: { co: String(++client), ct: String(now) },
    s: { s: wallet.address, r: signature.r, s1: signature.s, v: signature.v, e: message.expiration, n: 123, ci: '325' },
  } };
}
const validate = (body, supplied = headers) => venue.validateRequest({ method: 'POST', url: REST + '/lite/v1/create_order', body: JSON.stringify(body), headers: supplied });
test('GRVT independently recovers a valid EIP-712 builder order and refuses economic mutations', async () => {
  const body = await signed();
  assert.doesNotThrow(() => validate(body));
  for (const mutate of [
    (o) => { o.l[0].s = '0.005'; }, (o) => { o.l[0].lp = '91'; },
    (o) => { o.l[0].i = 'ETH_USDT_Perp'; }, (o) => { o.l[0].ib = false; },
    (o) => { o.ro = true; }, (o) => { o.po = true; }, (o) => { o.bf = '0.02'; },
    (o) => { o.s.ci = '326'; }, (o) => { o.s.n++; },
    (o) => { o.l[0].s = '0.0000000001'; },
  ]) {
    const changed = structuredClone(body); mutate(changed.o);
    assert.throws(() => validate(changed));
  }
});
test('GRVT validates login, real upstreams, private session/account and unchanged auth resends', async () => {
  const body = await signed();
  assert.throws(() => validate(body, {}), /session/);
  assert.throws(() => validate(body, { ...headers, 'X-Grvt-Account-Id': 'wrong' }), /session/);
  assert.throws(() => venue.validateRequest({ method: 'POST', url: 'https://proxy.invalid/lite/v1/create_order', body, headers }), /upstream/);
  assert.throws(() => venue.validateRequest({ method: 'POST', url: 'https://edge.grvt.io/auth/api_key/login', body: '{"api_key":"wrong"}', headers: { 'X-Grvt-Remember-Me': 'true' } }), /credentials/);
  assert.doesNotThrow(() => validate(body));
  assert.doesNotThrow(() => validate(body, { ...headers, 'X-Ts-Grvt-Session': 'fixture-session-2', Cookie: 'gravity=fixture-session-2' }));
  const changed = structuredClone(body); changed.o.m.ct = String(BigInt(changed.o.m.ct) + 1000000n);
  assert.throws(() => validate(changed), /resend changed/);
});
test('GRVT matching removes only verified signature entropy and clocks, retaining identities, presence and economics', async () => {
  const body = await signed();
  const key = (path, data) => requestKey('POST', REST + path, data, new Set([body.o.m.co]), venue);
  const other = structuredClone(body);
  for (const field of ['r', 's1', 'v', 'e', 'n']) other.o.s[field] = 'different';
  other.o.m.ct = 'different';
  assert.equal(key('/lite/v1/create_order', body), key('/lite/v1/create_order', other));
  for (const mutate of [
    (o) => { o.m.co = 'different'; }, (o) => { o.l[0].s = '0.005'; },
    (o) => { o.s.s = 'different'; }, (o) => { o.s.ci = '326'; },
    (o) => { delete o.s.r; }, (o) => { o.bf = '0.04'; },
  ]) {
    const changed = structuredClone(body); mutate(changed.o);
    assert.notEqual(key('/lite/v1/create_order', body), key('/lite/v1/create_order', changed));
  }
  assert.notEqual(key('/lite/v1/cancel_order', { sa: '777', oi: '0x300' }), key('/lite/v1/cancel_order', { sa: '777', oi: '0x301' }));
});
test('GRVT Tape persists rejected signatures after valid retries and consumes write acknowledgements once', async () => {
  const body = await signed();
  const records = [{ type: 'http', method: 'POST', url: REST + '/lite/v1/create_order', status: 200, body: '{"r":{"oi":"0x300"}}', requestBody: body }];
  const tape = new Tape(records, venue, true, 30000, 'rust');
  const changed = structuredClone(body); changed.o.l[0].s = '0.005';
  assert.throws(() => tape.respond('POST', records[0].url, JSON.stringify(changed), headers));
  assert.equal(tape.respond('POST', records[0].url, JSON.stringify(body), headers).status, 200);
  assert.equal(tape.invalidSignatures, 1);
  assert.equal(tape.respond('POST', records[0].url, JSON.stringify(body), headers).status, 599);
});
test('GRVT protocol audit requires credentials first, waits for auth ACK, checks exact selectors and pongs', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate: true, books: [] });
  const auth = { type: 'auth', session_cookie: 'rm=true; gravity=fixture-session-1', account_id: 'fixture-account', testnet: false };
  audit.sent(PRIVATE, JSON.stringify(auth));
  assert.equal(audit.ready(), true);
  audit.received(PRIVATE, '{"type":"auth_success"}');
  for (const stream of ['v1.order', 'v1.position', 'v1.fill']) audit.sent(PRIVATE, JSON.stringify({ request_id: 1, method: 'subscribe', stream, feed: ['777'], is_full: false }));
  audit.received(PRIVATE, '{"s":"v1.fill","f":{}}');
  audit.received(PRIVATE, 'ping'); audit.sent(PRIVATE, 'pong');
  assert.deepEqual(audit.errors(), []);
  const bad = venue.createSocketAudit('rust', { isPrivate: true, books: [] });
  bad.sent(PRIVATE, JSON.stringify({ method: 'subscribe', stream: 'v1.fill', feed: ['778'], is_full: false }));
  bad.received(PRIVATE, '{"type":"auth_success"}');
  bad.received(PRIVATE, 'ping');
  assert.ok(bad.errors().some((e) => /preceded auth|preceded credentials/.test(e)));
  assert.ok(bad.errors().some((e) => /missing GRVT pong/.test(e)));
});

test('GRVT public market requests reject leaked private credentials', () => {
  const request = { method: 'POST', url: 'https://market-data.grvt.io/lite/v1/all_instruments', body: '{}' };
  assert.doesNotThrow(() => venue.validateRequest(request));
  for (const header of ['Cookie', 'X-Ts-Grvt-Session', 'X-Grvt-Account-Id', 'X-API-KEY']) {
    assert.throws(() => venue.validateRequest({ ...request, headers: { [header]: 'private' } }), /private credentials/);
  }
});
