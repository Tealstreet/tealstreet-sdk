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
async function signed(instrument = 'BTC_USDT_Perp', hash = '0x030501') {
  const now = BigInt(Date.now()) * 1000000n;
  const message = { subAccountID: '777', isMarket: false, timeInForce: 1, postOnly: false, reduceOnly: false,
    legs: [{ assetID: hash, contractSize: '4000000', limitPrice: '90000000000', isBuyingContract: true }],
    builder: '0xee8f3559f298fffa79b8253437969d15b5dccff3', builderFee: 300, nonce: 123,
    expiration: String(now + 3600000000000n) };
  const signature = Signature.from(await wallet.signTypedData({ name: 'GRVT Exchange', version: '0', chainId: 325 }, types, message));
  return { o: { sa: '777', im: false, ti: 'GOOD_TILL_TIME', po: false, ro: false,
    l: [{ i: instrument, s: '0.004', lp: '90', ib: true }],
    b: message.builder, bf: '0.03', m: { co: String(++client), ct: String(now) },
    s: { s: wallet.address, r: signature.r, s1: signature.s, v: signature.v, e: message.expiration, n: 123, ci: '325' },
  } };
}
const validate = (body, supplied = headers) => venue.validateRequest({ method: 'POST', url: REST + '/lite/v1/create_order', body: JSON.stringify(body), headers: supplied });
const LOGIN = 'https://edge.grvt.io/auth/api_key/login';
const loginHeaders = { 'X-Grvt-Remember-Me': 'true' };
const loginBody = '{"api_key":"grvt-replay-key"}';
const loginRecord = (index = 1, afterCommand = 0) => ({
  type: 'http', method: 'POST', url: LOGIN, status: 200, body: '{"status":"success"}',
  requestBody: { api_key: 'grvt-replay-key' }, afterCommand,
  headers: `Set-Cookie: gravity=fixture-session-${index}; Path=/\nX-Ts-Grvt-Session: fixture-session-${index}\nX-Grvt-Account-Id: fixture-account`,
});
const login = (tape) => tape.respond('POST', LOGIN, loginBody, loginHeaders);
const sessionHeaders = (index) => ({ ...headers, 'X-Ts-Grvt-Session': `fixture-session-${index}`, Cookie: `gravity=fixture-session-${index}` });

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
  const tape = new Tape([loginRecord(), ...records], venue, true, 30000, 'rust');
  assert.equal(login(tape).status, 200);
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


test('GRVT Tape rotates credentials only from actual login responses and invalidates rejected sessions persistently', () => {
  const url = REST + '/lite/v1/positions', body = '{"sa":"777"}';
  const records = [loginRecord(), loginRecord(2, 1),
    { type: 'http', method: 'POST', url, requestBody: { sa: '777' }, status: 200, body: '{"r":[]}' },
    { type: 'http', method: 'POST', url, requestBody: { sa: '777' }, status: 401, body: '{"c":1000,"m":"expired"}', afterCommand: 1 },
    { type: 'http', method: 'POST', url, requestBody: { sa: '777' }, status: 200, body: '{"r":[]}', afterCommand: 2 },
  ];
  const tape = new Tape(records, venue, true, 30000, 'rust');
  assert.equal(login(tape).status, 200);
  assert.equal(tape.respond('POST', url, body, sessionHeaders(1)).status, 200);
  tape.commandIndex = 1;
  assert.equal(tape.respond('POST', url, body, sessionHeaders(1)).status, 401);
  assert.throws(() => tape.respond('POST', url, body, sessionHeaders(1)), /current authenticated session/);
  assert.throws(() => tape.respond('POST', url, body, sessionHeaders(2)), /current authenticated session/);
  assert.equal(login(tape).status, 200);
  assert.throws(() => tape.respond('POST', url, body, sessionHeaders(1)), /current authenticated session/);
  tape.commandIndex = 2;
  assert.equal(tape.respond('POST', url, body, sessionHeaders(2)).status, 200);
  assert.equal(tape.invalidSignatures, 3);
  assert.throws(() => tape.assertWritesConsumed(), /3 invalid replay signature/);
});

test('GRVT private REST cannot authenticate itself before successful login, and Tape instances keep separate sessions', () => {
  const record = { type: 'http', method: 'POST', url: REST + '/lite/v1/positions', requestBody: { sa: '777' }, status: 200, body: '{"r":[]}' };
  const authenticated = new Tape([loginRecord(), record], venue, true, 30000, 'rust');
  assert.equal(login(authenticated).status, 200);
  const separate = new Tape([loginRecord(), record], venue, true, 30000, 'rust');
  assert.throws(() => separate.respond('POST', record.url, '{"sa":"777"}', headers), /current authenticated session/);
  assert.equal(separate.invalidSignatures, 1);
  assert.equal(authenticated.respond('POST', record.url, '{"sa":"777"}', headers).status, 200);
});

test('GRVT Tape requires first-frame auth per real socket generation and rejects same-socket reauthentication permanently', async () => {
  const tape = new Tape([loginRecord(), loginRecord(2, 1)], venue, true, 30000, 'rust');
  assert.equal(login(tape).status, 200);
  const connect = async () => {
    const socket = tape.connect(PRIVATE, () => {}, () => {}, () => {});
    await new Promise(setImmediate);
    return socket;
  };
  const auth = (socket, index) => tape.send(socket, JSON.stringify({ type: 'auth',
    session_cookie: `rm=true; gravity=fixture-session-${index}`, account_id: 'fixture-account', testnet: false }));
  const acknowledge = (socket) => {
    tape.deliver(socket, '{"type":"auth_success"}');
    for (const stream of ['v1.order', 'v1.position', 'v1.fill']) tape.send(socket,
      JSON.stringify({ method: 'subscribe', stream, feed: ['777'], is_full: false }));
  };
  const first = await connect();
  auth(first, 1); acknowledge(first);
  assert.deepEqual(tape.audit.errors(), []);
  tape.commandIndex = 1;
  assert.equal(login(tape).status, 200);
  auth(first, 2);
  assert.ok(tape.audit.errors().some((error) => /first-frame auth for socket generation/.test(error)));
  tape.close(first);
  const second = await connect();
  assert.equal(second.generation, first.generation + 1);
  auth(second, 2); acknowledge(second);
  // Correct authentication on a new connection cannot erase the earlier fault.
  assert.deepEqual(tape.audit.errors(), ['invalid GRVT first-frame auth for socket generation']);
});

test('GRVT socket generation must authenticate with the current REST session and cannot send heartbeat first', async () => {
  for (const firstFrame of ['ping', JSON.stringify({ type: 'auth', session_cookie: 'rm=true; gravity=fixture-session-2', account_id: 'fixture-account', testnet: false })]) {
    const tape = new Tape([loginRecord()], venue, true, 30000, 'rust');
    assert.equal(login(tape).status, 200);
    const socket = tape.connect(PRIVATE, () => {}, () => {}, () => {});
    await new Promise(setImmediate);
    tape.send(socket, firstFrame);
    assert.ok(tape.audit.errors().some((error) => /first.frame/.test(error)));
  }
});


test('GRVT session rejection invalidates HTTP-200 lite and full error aliases', () => {
  for (const error of [{ c: 1000 }, { code: '1000' }, { e: { c: '1000' } }, { e: { code: 1000 } }, { error: { c: 1000 } }, { error: { code: '1000' } }]) {
    const url = REST + '/lite/v1/positions';
    const tape = new Tape([loginRecord(), { type: 'http', method: 'POST', url,
      requestBody: { sa: '777' }, status: 200, body: JSON.stringify(error) }], venue, true, 30000, 'rust');
    assert.equal(login(tape).status, 200);
    assert.equal(tape.respond('POST', url, '{"sa":"777"}', headers).status, 200);
    assert.throws(() => tape.respond('POST', url, '{"sa":"777"}', headers), /current authenticated session/);
    assert.equal(tape.invalidSignatures, 1);
  }
});

test('GRVT indexed public subscriptions match the original prefix scan through mutations and generations', () => {
  const url = 'wss://market-data.grvt.io/ws';
  const audit = venue.createSocketAudit('rust', { isPrivate: false, books: [] });
  const oldSets = new Map(), expected = [];
  const instruments = ['BTC', 'BTC@nested', 'BTC@nested@', '', null, 0, false, undefined, 'undefined', 'other'];
  const streams = ['v1.book.s', 'v1.book.s:extra', 'v1.book.s@nested'];
  const operations = [
    [1, true, streams[0], ['BTC@500-50', 'BTC@500-50', 'BTC@100-10', 'BTC@nested@500', '@', 'null@', '0@', 'false@', 'undefined@']],
    [1, true, streams[1], ['BTC@500']], [1, true, streams[2], ['BTC@nested@@500']],
    [2, true, streams[0], ['other@500']],
    [1, false, streams[0], ['BTC@500-50', 'absent@500']],
    [1, false, streams[0], ['BTC@100-10']],
    [1, false, streams[0], ['BTC@nested@500']],
    [1, true, streams[0], ['BTC@nested@500', null, 0, false, 'no-delimiter']],
    [1, false, streams[0], ['BTC@nested@500', 'BTC@nested@500']],
    [2, false, streams[0], ['other@500']],
  ];
  for (const [generation, add, stream, selectors] of operations) {
    if (!oldSets.has(generation)) oldSets.set(generation, new Set());
    const old = oldSets.get(generation);
    for (const selector of selectors) {
      const key = `${stream}:${selector}`;
      if (add) old.add(key); else old.delete(key);
    }
    audit.sent(url, JSON.stringify({ method: add ? 'subscribe' : 'unsubscribe', params: { stream, selectors } }), generation);
    for (const gen of [1, 2, 3]) for (const s of streams) for (const instrument of instruments) {
      const text = JSON.stringify({ s, f: { i: instrument } });
      const feed = JSON.parse(text).f, value = feed.i ?? feed.instrument;
      const accepted = [...(oldSets.get(gen) ?? [])].some((key) => key.startsWith(`${s}:${value}@`));
      if (!accepted) expected.push(`unsubscribed GRVT public data: ${s}:${value}`);
      audit.received(url, text, gen);
    }
    assert.deepEqual(audit.errors(), expected);
    assert.deepEqual(audit.errors(), expected, 'reading errors must preserve accumulated failure order');
  }
  audit.closed(url, 1);
  audit.received(url, '{"s":"v1.book.s","f":{"i":"BTC"}}', 1);
  assert.deepEqual(audit.errors().slice(-2), [
    'GRVT received a frame on a closed socket generation', 'unsubscribed GRVT public data: v1.book.s:BTC',
  ]);
});

test('GRVT hostname caching is lazy, generation-local and leaves coercible URLs uncached', () => {
  const NativeURL = globalThis.URL;
  let parses = 0;
  globalThis.URL = class extends NativeURL { constructor(...args) { super(...args); parses++; } };
  try {
    const audit = venue.createSocketAudit('rust', { isPrivate: false, books: [] });
    const url = 'wss://market-data.grvt.io/ws';
    audit.connected(url, 1); audit.closed(url, 9);
    audit.received(url, 'ping', 1); audit.received(url, 'pong', 1);
    assert.equal(parses, 0);
    audit.sent(url, 'pong', 1);
    assert.equal(parses, 1);
    audit.sent(url, '{"method":"subscribe","params":{"stream":"book","selectors":["BTC@500"]}}', 1);
    for (let i = 0; i < 10; i++) audit.received(url, '{"s":"book","f":{"i":"BTC"}}', 1);
    audit.errors(); audit.errors();
    assert.equal(parses, 2, 'only the separately closed generation needs another parse');
    audit.received(url, '{}', 2);
    assert.equal(parses, 3);
    let address = url;
    const mutable = { toString: () => address };
    audit.received(mutable, '{}', 3);
    assert.equal(parses, 5, 'both original receive call sites parse non-string inputs');
    address = PRIVATE;
    audit.received(mutable, '{"s":"v1.fill","f":{}}', 3);
    assert.equal(parses, 7);
    assert.ok(audit.errors().includes('unsubscribed GRVT private data: v1.fill'));
  } finally { globalThis.URL = NativeURL; }
});

test('GRVT lazy URL parsing preserves invalid URL, malformed JSON and heartbeat error ordering', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate: false, books: [] });
  const bad = 'not-a-url';
  assert.doesNotThrow(() => audit.connected(bad, 1));
  assert.doesNotThrow(() => audit.closed(bad, 2));
  assert.doesNotThrow(() => audit.received(bad, 'ping', 1));
  assert.doesNotThrow(() => audit.received(bad, 'pong', 1));
  assert.throws(() => audit.received(bad, '{', 1), SyntaxError);
  assert.throws(() => audit.received(bad, '{}', 1), { code: 'ERR_INVALID_URL' });
  assert.throws(() => audit.sent(bad, '{', 1), { code: 'ERR_INVALID_URL' });
  assert.throws(() => audit.sent(bad, 'pong', 1), { code: 'ERR_INVALID_URL' });
  assert.throws(() => audit.errors(), { code: 'ERR_INVALID_URL' });
  // A failed URL parse in sent must not consume the first-auth message count.
  let coercions = 0;
  const mutable = { toString: () => ++coercions === 2 ? bad : PRIVATE };
  const auth = venue.createSocketAudit('rust', { isPrivate: true, books: [] });
  assert.throws(() => auth.sent(mutable, '{}'), { code: 'ERR_INVALID_URL' });
  auth.sent(mutable, '{"type":"auth","session_cookie":"rm=true; gravity=fixture-session-1","account_id":"fixture-account","testnet":false}');
  assert.equal(auth.ready(), true);
});
