import test, { before } from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import {
  APEX_ACCOUNT, apexSignatureForTest, apexWireBody, createApexSocketAudit,
  prepareApexReplay, validateApexRequest,
} from './apexomni-protocol.mjs';

before(() => prepareApexReplay({ isPrivate: true }));
const now = 1792454400123;
const signature = (text) => createHmac('sha256', Buffer.from('replay-secret').toString('base64')).update(text).digest('base64');
const form = (fields) => Object.entries(fields).sort(([a], [b]) => a.localeCompare(b)).map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(value)}`).join('&');
function request(path, fields, timestamp = now) {
  const body = form(fields);
  return { method: 'POST', url: `https://omni.apex.exchange/api${path}`, body, now, callerIds: new Set(['caller-exact-01', 'apexomni-custom']), headers: {
    'APEX-API-KEY': 'replay-key', 'APEX-PASSPHRASE': 'replay-passphrase',
    'APEX-TIMESTAMP': String(timestamp), 'APEX-SIGNATURE': signature(`${timestamp}POST/api${path}${body}`),
    'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8',
  } };
}
const parent = () => {
  const id = 'caller-exact-01';
  return { brokerId: '6587', clientId: id, clientOrderId: id,
    expiration: (Math.ceil((now + 30 * 86400000) / 3600000) + 168) * 3600000,
    limitFee: '0.030000', price: '60000', side: 'BUY', size: '0.001', symbol: 'BTC-USDT',
    timeInForce: 'GOOD_TIL_CANCEL', type: 'LIMIT', signature: apexSignatureForTest(id, '0.001', '60000', true),
  };
};
test('ApeX independent SDK verifies a caller ID parent and exact millisecond API auth', () => {
  assert.doesNotThrow(() => validateApexRequest(request('/v3/order', parent())));
});
test('ApeX rejects tampered L2 signature even when the API HMAC is valid', () => {
  const fields = parent(); fields.signature = '0'.repeat(128);
  assert.throws(() => validateApexRequest(request('/v3/order', fields)), /parent L2 signature/);
});
test('ApeX binds quantity, price, client ID and direction in the real SDK signature', () => {
  for (const [key, value] of [['size', '0.002'], ['price', '60001'], ['clientId', 'other'], ['side', 'SELL']]) {
    const fields = parent(); fields[key] = value;
    if (key === 'clientId') fields.clientOrderId = value;
    if (key === 'size') fields.limitFee = '0.060000';
    if (key === 'price') fields.limitFee = '0.030001';
    const r = request('/v3/order', fields); r.callerIds.add('other');
    assert.throws(() => validateApexRequest(r), /L2 signature/);
  }
});
test('ApeX rejects altered broker, fee, expiry and mismatched caller ID before acceptance', () => {
  for (const [key, value] of [['brokerId', 'tealstreet'], ['limitFee', '0.030001'], ['expiration', now], ['clientOrderId', 'rewritten']]) {
    const fields = parent(); fields[key] = value;
    assert.throws(() => validateApexRequest(request('/v3/order', fields)));
  }
});
test('ApeX preserves caller IDs that happen to start with the generated-ID prefix', () => {
  const fields = parent(); fields.clientId = fields.clientOrderId = 'apexomni-custom';
  fields.signature = apexSignatureForTest(fields.clientId, fields.size, fields.price, true);
  assert.doesNotThrow(() => validateApexRequest(request('/v3/order', fields)));
  const r = request('/v3/order', fields); r.callerIds.clear();
  assert.throws(() => validateApexRequest(r), /generated client order ID/);
});
test('ApeX bracket size and independent opposite-side signature are verified', () => {
  const fields = parent();
  const id = `apexomni-${APEX_ACCOUNT}-${now}-123456`;
  Object.assign(fields, { isOpenTpslOrder: 'true', isSetOpenTp: 'true', tpClientOrderId: id,
    tpExpiration: fields.expiration, tpLimitFee: '0.033333', tpPrice: '66665', tpSide: 'SELL',
    tpSignature: apexSignatureForTest(id, fields.size, '66665', false), tpSize: fields.size,
    tpTriggerPrice: '67000', tpTriggerPriceType: 'MARKET' });
  assert.doesNotThrow(() => validateApexRequest(request('/v3/order', fields)));
  fields.tpSignature = '0'.repeat(128);
  assert.throws(() => validateApexRequest(request('/v3/order', fields)), /bracket L2 signature/);
  fields.tpSignature = apexSignatureForTest(id, fields.size, '66665', false);
  fields.tpSize = '0.002';
  assert.throws(() => validateApexRequest(request('/v3/order', fields)), /bracket size/);
});
test('ApeX API auth uses base64 text as HMAC key and refuses raw-secret signing', () => {
  const r = request('/v3/delete-order', { id: 'order' });
  r.headers['APEX-SIGNATURE'] = createHmac('sha256', 'replay-secret').update(`${now}POST/api/v3/delete-orderid=order`).digest('base64');
  assert.throws(() => validateApexRequest(r), /REST HMAC/);
});
test('ApeX one-second legacy timestamp preserves canonical cancellation body', () => {
  assert.doesNotThrow(() => validateApexRequest(request('/v3/delete-order', { id: 'order' }, Math.floor(now / 1000) * 1000)));
});
test('ApeX quantized legacy auth preserves order expiry across the hour boundary', () => {
  const fields = parent(); fields.clientId = fields.clientOrderId = 'legacy-hour-caller';
  fields.signature = apexSignatureForTest(fields.clientId, fields.size, fields.price, true);
  const r = request('/v3/order', fields, Math.floor(now / 1000) * 1000);
  r.callerIds.add(fields.clientId);
  assert.doesNotThrow(() => validateApexRequest(r));
  fields.expiration += 3600000;
  const changed = request('/v3/order', fields, Math.floor(now / 1000) * 1000);
  changed.callerIds.add(fields.clientId);
  assert.throws(() => validateApexRequest(changed), /parent expiration/);
});
test('ApeX query sorting signs canonical RFC3986 values instead of insertion order', () => {
  const payload = 'limit=100&symbol=BTC-USDT';
  const r = { method: 'GET', url: 'https://omni.apex.exchange/api/v3/fills?symbol=BTC-USDT&limit=100', now, headers: {
    'APEX-API-KEY': 'replay-key', 'APEX-PASSPHRASE': 'replay-passphrase', 'APEX-TIMESTAMP': String(now),
    'APEX-SIGNATURE': signature(`${now}GET/api/v3/fills?${payload}`),
  } };
  assert.doesNotThrow(() => validateApexRequest(r));
});
test('ApeX klines are unsigned and a proxy URL cannot pass protocol validation', () => {
  assert.doesNotThrow(() => validateApexRequest({ method: 'GET', url: 'https://omni.apex.exchange/api/v3/klines', headers: {} }));
  assert.throws(() => validateApexRequest({ method: 'GET', url: 'https://omni.apex.exchange/api/v3/klines', headers: { 'APEX-API-KEY': 'bad' } }), /unauthenticated/);
  const r = request('/v3/delete-order', { id: 'order' }); r.url = r.url.replace('omni.apex.exchange', 'proxy.example');
  assert.throws(() => validateApexRequest(r), /real upstream/);
});
test('ApeX multipart parsing retains economic fields and rejects duplicates or truncation', () => {
  const body = '--boundary\r\nContent-Disposition: form-data; name="symbol"\r\n\r\nBTC-USDT\r\n--boundary--\r\n';
  assert.deepEqual(apexWireBody(body), { symbol: 'BTC-USDT' });
  assert.throws(() => apexWireBody(body.slice(0, -8)), /terminator/);
  assert.throws(() => apexWireBody(body.replace('--boundary--', '--boundary\r\nContent-Disposition: form-data; name="symbol"\r\n\r\nETH-USDT\r\n--boundary--')), /duplicate/);
});
test('ApeX requires the authenticated leverage body to match its multipart header boundary', () => {
  const fields = { symbol: 'BTC-USDT', token: 'USDT', initialMarginRate: '0.05000' };
  const r = request('/v3/set-initial-margin-rate', fields);
  r.body = Object.entries(fields).map(([key, value]) => `--actual\r\nContent-Disposition: form-data; name="${key}"\r\n\r\n${value}\r\n`).join('') + '--actual--\r\n';
  r.headers['Content-Type'] = 'multipart/form-data; boundary=actual';
  assert.doesNotThrow(() => validateApexRequest(r));
  r.headers['Content-Type'] = 'multipart/form-data; boundary=wrong';
  assert.throws(() => validateApexRequest(r), /boundary differs/);
});
function login(time = Date.now()) {
  return JSON.stringify({ op: 'login', args: [JSON.stringify({ type: 'login', httpMethod: 'GET', requestPath: '/ws/accounts', apiKey: 'replay-key', passphrase: 'replay-passphrase', timestamp: time, signature: signature(`${time}GET/ws/accounts`) })] });
}
test('ApeX socket audit requires nested login before all three private subscriptions', () => {
  const url = 'wss://quote.omni.apex.exchange/realtime_private?v=2';
  const a = createApexSocketAudit('rust', { isPrivate: true }); a.connected(url);
  a.sent(url, login());
  a.sent(url, '{"op":"subscribe","args":["ws_zk_accounts_v3","ws_user_v1","ws_notify_v1"]}');
  a.sent(url, '{"op":"ping"}');
  a.received(url, '{"op":"ping","args":["server-token",7]}');
  a.sent(url, '{"op":"pong","args":["server-token",7]}');
  assert.deepEqual(a.errors(), []);
  const wrong = createApexSocketAudit('ts', { isPrivate: true }); wrong.connected(url);
  wrong.sent(url, '{"op":"subscribe","args":["ws_zk_accounts_v3"]}');
  assert.ok(wrong.errors().some((e) => e.includes('preceded login')));
});
test('ApeX socket reconnect must authenticate again and answer its own server ping', () => {
  const url = 'wss://quote.omni.apex.exchange/realtime_private?v=2';
  const a = createApexSocketAudit('rust', { isPrivate: true }); a.connected(url); a.sent(url, login());
  a.sent(url, '{"op":"subscribe","args":["ws_zk_accounts_v3","ws_user_v1","ws_notify_v1"]}');
  a.sent(url, '{"op":"ping"}'); a.connected(url);
  a.received(url, '{"op":"ping","args":["server-token"]}');
  assert.ok(a.errors().some((e) => e.includes('not answered')));
  assert.ok(a.errors().some((e) => e.includes('did not restore')));
});
test('ApeX server ping without args requires one fresh decimal timestamp string', () => {
  const url = 'wss://quote.omni.apex.exchange/realtime_public?v=2';
  for (const args of [[], ['bad'], [0], [String(Date.now()), 'extra']]) {
    const a = createApexSocketAudit('rust', { isPrivate: false }); a.connected(url); a.sent(url, '{"op":"ping"}');
    a.received(url, '{"op":"ping"}'); a.sent(url, JSON.stringify({ op: 'pong', args }));
    assert.ok(a.errors().some((e) => e.includes('fallback timestamp')));
  }
  const a = createApexSocketAudit('ts', { isPrivate: false }); a.connected(url); a.sent(url, '{"op":"ping"}');
  a.received(url, '{"op":"ping"}'); a.sent(url, JSON.stringify({ op: 'pong', args: [String(Date.now())] }));
  assert.deepEqual(a.errors(), []);
});


test('ApeX wire JSON cannot collapse adjacent unsafe numeric order identities in any numeric spelling', async () => {
  const { requestKey } = await import('./requests.mjs');
  const url = 'https://omni.apex.exchange/api/v3/delete-order';
  const options = { wireBody: apexWireBody };
  const key = (literal) => requestKey('POST', url, `{"id":${literal},"clientId":"caller-exact-01"}`, new Set(['caller-exact-01']), options);
  for (const [first, second] of [
    ['9007199254740992', '9007199254740993'],
    ['9007199254740992.0', '9007199254740993.0'],
    ['9007199254740992e0', '9007199254740993e0'],
    ['9.007199254740992e15', '9.007199254740993e15'],
  ]) {
    assert.notEqual(key(first), key(second), `${first} and ${second} are distinct wire identities`);
    assert.equal(apexWireBody(`{"id":${second}}`).id, second);
  }
});

test('ApeX wire fields retain raw JSON identity, exact caller strings and ordinary number serialization', () => {
  for (const literal of ['9007199254740993', '9007199254740993.0', '9007199254740993e0']) {
    const raw = JSON.rawJSON(literal);
    const body = { id: raw, clientId: 'apexomni-caller', size: 42, price: 1.25, exponent: 1e3 };
    assert.deepEqual(apexWireBody(body), { id: literal, clientId: 'apexomni-caller', size: '42', price: '1.25', exponent: '1000' });
    assert.equal(body.id, raw, 'normalization never mutates the input raw node');
  }
});

test('replay request validation receives the exact recorded caller-ID set and keeps failures persistent', async () => {
  const { Tape } = await import('./tape.mjs');
  const url = 'https://omni.apex.exchange/api/v3/user';
  const records = [
    { type: 'http', method: 'GET', url, status: 200, body: '{}' },
    { type: 'command', op: 'placeOrders', orders: [{ clientOrderId: 'apexomni-caller' }] },
  ];
  let valid = true;
  let captured;
  const tape = new Tape(records, { validateRequest({ callerIds }) {
    captured = callerIds;
    assert(callerIds.has('apexomni-caller'));
    return valid;
  } }, true);
  assert.equal(tape.respond('GET', url).status, 200);
  assert.equal(captured, tape.callerIds);
  tape.assertWritesConsumed();
  valid = false;
  assert.throws(() => tape.respond('GET', url), /invalid replay request validation/);
  assert.equal(tape.invalidSignatures, 1);
  assert.throws(() => tape.assertWritesConsumed(), /invalid replay signature/);
});
