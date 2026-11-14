import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { createPrivateKey, sign } from 'node:crypto';
import { COINBASE_REPLAY } from './coinbase-protocol.mjs';
import {
  DRB_HTTP, DRB_PRIVATE_SOCKET, coinbaseDrbIntxTape, createDrbSocketAudit, drbRequestBody, drbSocketReplies,
  isDrbWrite, isRustOnlyTape, verifyDrbRequest,
} from './coinbase-drb-tape.mjs';
import { rustOnlyOracle } from './rust-only-oracle.mjs';

const auditAuth = (id = 1) => {
  const nbf = Math.floor(Date.now() / 1000);
  const part = (value) => Buffer.from(JSON.stringify(value)).toString('base64url');
  const unsigned = `${part({ alg: 'EdDSA', typ: 'JWT', kid: COINBASE_REPLAY.key, nonce: 'synthetic-audit-nonce' })}.${part({ sub: COINBASE_REPLAY.key, iss: 'cdp', nbf, exp: nbf + 120 })}`;
  const key = createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.from(COINBASE_REPLAY.secret, 'base64')]), format: 'der', type: 'pkcs8' });
  return { jsonrpc: '2.0', id, method: 'public/auth', params: { grant_type: 'coinbase_cdp', token: `${unsigned}.${sign(null, Buffer.from(unsigned), key).toString('base64url')}` } };
};
const auditChannels = ['user.changes.future.USDC.100ms', 'user.portfolio.USDC', 'user.isolated.changes.future.USDC.100ms'];
const auditSubscribe = (id = 2) => ({ jsonrpc: '2.0', id, method: 'private/subscribe', params: { channels: auditChannels } });

test('drb audit requires correlated successful auth and subscription replies', () => {
  const audit = createDrbSocketAudit();
  const send = (message) => audit.sent(DRB_PRIVATE_SOCKET, JSON.stringify(message), 1);
  send(auditAuth());
  send(auditSubscribe());
  assert.equal(audit.ready(), false, 'writing requests is not successful authentication/subscription');
  assert.match(audit.errors().join('\n'), /subscribed before authenticating/);

  const good = createDrbSocketAudit();
  good.sent(DRB_PRIVATE_SOCKET, JSON.stringify(auditAuth()), 7);
  good.received(DRB_PRIVATE_SOCKET, JSON.stringify({ jsonrpc: '2.0', id: 99, result: {} }), 7);
  good.received(DRB_PRIVATE_SOCKET, JSON.stringify({ jsonrpc: '2.0', id: 1, result: {} }), 8);
  good.received(DRB_PRIVATE_SOCKET, JSON.stringify({ jsonrpc: '2.0', id: 1, result: { access_token: 'synthetic', expires_in: 900 } }), 7);
  good.sent(DRB_PRIVATE_SOCKET, JSON.stringify(auditSubscribe()), 7);
  assert.equal(good.ready(), false);
  good.received(DRB_PRIVATE_SOCKET, JSON.stringify({ jsonrpc: '2.0', id: 2, result: auditChannels }), 7);
  assert.equal(good.ready(), true);
});

test('drb audit never authenticates a rejected or invalid login', () => {
  for (const invalid of [false, true]) {
    const audit = createDrbSocketAudit();
    const auth = auditAuth();
    if (invalid) auth.params.token = 'invalid.jwt.token';
    audit.sent(DRB_PRIVATE_SOCKET, JSON.stringify(auth), 1);
    audit.received(DRB_PRIVATE_SOCKET, JSON.stringify({ jsonrpc: '2.0', id: 1, error: { code: 10000 } }), 1);
    audit.sent(DRB_PRIVATE_SOCKET, JSON.stringify(auditSubscribe()), 1);
    audit.received(DRB_PRIVATE_SOCKET, JSON.stringify({ jsonrpc: '2.0', id: 2, result: auditChannels }), 1);
    assert.equal(audit.ready(), false);
  }
});

// S0's closed classifier is the authority for which gateway methods are writes.
const classified = () => {
  const source = readFileSync(new URL('../venues/coinbase/src/drb.rs', import.meta.url), 'utf8');
  const body = source.slice(source.indexOf('fn method_intent'), source.indexOf('_ => return Err("drb unsupported method"'));
  const methods = new Map();
  const parts = body.split('=>');
  for (let index = 0; index + 1 < parts.length; index++) {
    const names = [...parts[index].matchAll(/"((?:public|private)\/\w+)"/g)].map((match) => match[1]);
    const intent = /^\s*Intent::(\w+)/.exec(parts[index + 1])?.[1];
    for (const name of names) methods.set(name, intent);
  }
  return methods;
};

test('drb write classification follows the S0 method classifier exactly', () => {
  const methods = classified();
  assert.ok(methods.size > 80);
  for (const [method, intent] of methods)
    assert.equal(isDrbWrite(`${DRB_HTTP}/${method}`), intent !== 'Read', `${method} is ${intent}`);
});

test('request identity drops only the RPC id and the separately verified auth JWT', () => {
  const url = `${DRB_HTTP}/public/auth`;
  assert.deepEqual(drbRequestBody({ jsonrpc: '2.0', id: 7, method: 'public/auth', params: { grant_type: 'coinbase_cdp', token: 'jwt' } }, { url }),
    { jsonrpc: '2.0', method: 'public/auth', params: { grant_type: 'coinbase_cdp', token: '<verified-jwt>' } });
  assert.deepEqual(drbRequestBody({ jsonrpc: '2.0', id: 8, method: 'private/get_positions', params: { currency: 'USDC' } }, { url: `${DRB_HTTP}/private/get_positions` }),
    { jsonrpc: '2.0', method: 'private/get_positions', params: { currency: 'USDC' } });
  const legacy = { id: 1, product_id: 'BTC-USD' };
  assert.equal(drbRequestBody(legacy, { url: 'https://api.coinbase.com/api/v3/brokerage/orders' }), legacy);
});

test('private reads need the issued bearer, public reads none, and auth a valid CDP JWT', () => {
  const read = (method, headers) => ({ method: 'POST', url: `${DRB_HTTP}/${method}`, headers, body: '{}' });
  assert.equal(verifyDrbRequest(read('private/get_positions', [['Authorization', 'Bearer replay-drb-token']])), true);
  assert.equal(verifyDrbRequest(read('private/get_positions', [['Authorization', 'Bearer other']])), false);
  assert.equal(verifyDrbRequest(read('private/get_positions', [])), false);
  assert.equal(verifyDrbRequest(read('public/get_instruments', [])), true);
  assert.equal(verifyDrbRequest(read('public/get_instruments', [['Authorization', 'Bearer replay-drb-token']])), false);
  const auth = JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'public/auth', params: { grant_type: 'coinbase_cdp', token: 'a.b.c' } });
  assert.equal(verifyDrbRequest({ method: 'POST', url: `${DRB_HTTP}/public/auth`, headers: [], body: auth }), false);
});

test('socket requests are answered by method with their own id, and readiness needs auth then every channel', () => {
  assert.deepEqual(drbSocketReplies('wss://advanced-trade-ws.coinbase.com', { jsonrpc: '2.0', id: 1, method: 'public/test' }), []);
  const [reply] = drbSocketReplies(DRB_PRIVATE_SOCKET, { jsonrpc: '2.0', id: 9, method: 'private/subscribe', params: { channels: ['a'] } });
  assert.deepEqual(JSON.parse(reply), { jsonrpc: '2.0', id: 9, result: ['a'] });
  const audit = createDrbSocketAudit();
  audit.connected(DRB_PRIVATE_SOCKET, 1);
  const channels = ['user.changes.future.USDC.100ms', 'user.portfolio.USDC', 'user.isolated.changes.future.USDC.100ms'];
  audit.sent(DRB_PRIVATE_SOCKET, JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'private/subscribe', params: { channels } }), 1);
  assert.match(audit.errors().join('\n'), /subscribed before authenticating/);
  assert.equal(audit.ready(), false);
});

test('the INTX tape is a deterministic Rust-only drb tape with no legacy routes', () => {
  for (const algorithm of ['ES256', 'EdDSA']) {
    const records = coinbaseDrbIntxTape({ algorithm });
    assert.deepEqual(records, coinbaseDrbIntxTape({ algorithm }));
    assert.equal(isRustOnlyTape(records), true);
    assert.equal(rustOnlyOracle(records, true), true);
    const urls = records.filter((r) => r.url).map((r) => new URL(r.url));
    assert.ok(urls.every((url) => ['drb.coinbase.com', 'api.coinbase.com'].includes(url.hostname)));
    assert.ok(urls.filter((url) => url.hostname === 'api.coinbase.com').every((url) => url.pathname.startsWith('/api/v3/brokerage/portfolios')));
    assert.ok(records.filter((r) => r.type === 'http').every((r) => !isDrbWrite(r.url)), 'S2 replays reads only');
  }
});


test('drb audit refuses mismatched, closed and invalid-login successful replies', () => {
  for (const mode of ['wrong-id', 'wrong-generation', 'closed', 'invalid-jwt', 'missing-channel']) {
    const audit = createDrbSocketAudit();
    const auth = auditAuth();
    if (mode === 'invalid-jwt') auth.params.token = 'invalid.jwt.token';
    audit.sent(DRB_PRIVATE_SOCKET, JSON.stringify(auth), 1);
    if (mode === 'closed') audit.closed(DRB_PRIVATE_SOCKET, 1);
    audit.received(DRB_PRIVATE_SOCKET, JSON.stringify({ jsonrpc: '2.0', id: mode === 'wrong-id' ? 99 : 1,
      result: { access_token: 'synthetic', expires_in: 900 } }), mode === 'wrong-generation' ? 2 : 1);
    audit.sent(DRB_PRIVATE_SOCKET, JSON.stringify(auditSubscribe()), 1);
    audit.received(DRB_PRIVATE_SOCKET, JSON.stringify({ jsonrpc: '2.0', id: 2,
      result: mode === 'missing-channel' ? auditChannels.slice(1) : auditChannels }), 1);
    assert.equal(audit.ready(), false, mode);
  }
});
