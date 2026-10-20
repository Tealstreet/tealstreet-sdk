import assert from 'node:assert/strict';
import test from 'node:test';
import { createPrivateKey, sign } from 'node:crypto';
import { COINBASE_REPLAY, COINBASE_ES256_REPLAY, verifyCoinbaseJwt, verifyCoinbaseRequest,
  verifyCoinbaseSubscribe, createCoinbaseSocketAudit } from './coinbase-protocol.mjs';

const NOW = 1_800_000_000_000;
const edKey = createPrivateKey({ key: Buffer.concat([
  Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.alloc(32, 13),
]), format: 'der', type: 'pkcs8' });
const jwt = ({ algorithm = 'EdDSA', uri, ...changes } = {}) => {
  const encode = (value) => Buffer.from(JSON.stringify(value)).toString('base64url');
  const header = { alg: algorithm, kid: COINBASE_REPLAY.key, typ: 'JWT', nonce: 'unique-nonce-123456' };
  const claims = { iss: algorithm === 'ES256' ? 'coinbase-cloud' : 'cdp',
    ...(algorithm === 'EdDSA' ? { aud: ['cdp_service'] } : {}), sub: COINBASE_REPLAY.key,
    nbf: NOW / 1000, exp: NOW / 1000 + 120, ...(uri === undefined ? {} : { uri }), ...changes };
  const payload = `${encode(header)}.${encode(claims)}`;
  const signature = sign(algorithm === 'ES256' ? 'sha256' : null, Buffer.from(payload),
    algorithm === 'ES256' ? { key: COINBASE_ES256_REPLAY.secret, dsaEncoding: 'ieee-p1363' } : edKey);
  return `${payload}.${signature.toString('base64url')}`;
};

test('both native algorithms require exact URI/time/claims and independent signature verification', () => {
  for (const algorithm of ['EdDSA', 'ES256']) {
    const token = jwt({ algorithm, uri: 'GET api.coinbase.com/api/v3/brokerage/orders' });
    const context = { algorithm, method: 'GET', path: '/api/v3/brokerage/orders?limit=100', now: NOW };
    assert.equal(verifyCoinbaseJwt(token, context), true);
    assert.equal(verifyCoinbaseJwt(token, { ...context, method: 'POST' }), false);
    assert.equal(verifyCoinbaseJwt(token, { ...context, path: '/api/v3/brokerage/portfolios' }), false);
    assert.equal(verifyCoinbaseJwt(token, { ...context, now: NOW + 10_000 }), false);
    assert.equal(verifyCoinbaseJwt(token, { now: NOW }), false);
    const parts = token.split('.');
    parts[1] = Buffer.from('{}').toString('base64url');
    assert.equal(verifyCoinbaseJwt(parts.join('.'), context), false);
  }
});

test('valid signatures with wrong semantic claims still fail', () => {
  for (const changes of [{ iss: 'wrong' }, { sub: 'wrong' }, { exp: NOW / 1000 + 121 },
    { nbf: -1 }, { aud: [] }, { uri: 'GET proxy.invalid/path' }]) {
    assert.equal(verifyCoinbaseJwt(jwt(changes), { now: NOW }), false);
  }
  const token = jwt();
  const parts = token.split('.');
  parts[0] += '=';
  assert.equal(verifyCoinbaseJwt(parts.join('.'), { now: NOW }), false);
});

test('request audit binds canonical venue URI and accepts the frozen TS proxy header alias', () => {
  const token = jwt({ uri: 'GET api.coinbase.com/api/v3/brokerage/portfolios' });
  const request = { method: 'GET', url: 'https://api.coinbase.com/api/v3/brokerage/portfolios?limit=100', now: NOW };
  for (const name of ['Authorization', 'Coinbase-Authorization']) {
    assert.equal(verifyCoinbaseRequest({ ...request, headers: { [name]: `Bearer ${token}` } }), true);
  }
  assert.equal(verifyCoinbaseRequest({ ...request, headers: {} }), false);
  assert.equal(verifyCoinbaseRequest({ ...request, headers: { Authorization: `Bearer ${token}`, 'Coinbase-Authorization': `Bearer ${token}` } }), false);
  assert.equal(verifyCoinbaseRequest({ ...request, url: 'https://wrong.invalid/api/v3/brokerage/portfolios', headers: {} }), false);
});

test('socket audit checks fresh JWT per private channel and no private auth on public sockets', () => {
  const oldNow = Date.now;
  Date.now = () => NOW;
  try {
    const privateUrl = 'wss://advanced-trade-ws-user.coinbase.com';
    const publicUrl = 'wss://advanced-trade-ws.coinbase.com';
    const audit = createCoinbaseSocketAudit('rust', { isPrivate: true, books: [] });
    audit.connected(privateUrl);
    audit.connected(publicUrl);
    const token = jwt();
    const user = { type: 'subscribe', channel: 'user', jwt: token };
    assert.equal(verifyCoinbaseSubscribe(user), true);
    audit.sent(privateUrl, JSON.stringify(user));
    audit.sent(privateUrl, JSON.stringify({ ...user, channel: 'heartbeats' }));
    audit.sent(publicUrl, JSON.stringify({ type: 'subscribe', channel: 'heartbeats', product_ids: [] }));
    assert.equal(audit.ready(), true);
    assert.deepEqual(audit.errors(), ['Coinbase reused a subscription JWT']);
    assert.equal(verifyCoinbaseSubscribe({ type: 'subscribe', channel: 'user' }), false);
    assert.equal(verifyCoinbaseSubscribe({ ...user, jwt: jwt({ uri: 'GET api.coinbase.com/path' }) }), false);
  } finally { Date.now = oldNow; }
});
