import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import {
  MAX_PER_HOST, REDACTED, RULES, capture, checkCases, fixture, isSensitive, redactBody, redactEndpoint,
} from './error-capture.mjs';

const expect = { kind: 'authInvalid', outcome: 'rejected', path: 'startup' };

// The Rust loader's the_loader_agrees_with_the_writer_on_the_shared_vectors reads
// the same vectors, so writer and re-check cannot drift apart.
test('every shared vector redacts to exactly its expected text, and keep texts are untouched', () => {
  for (const { raw, redacted } of RULES.vectors.redact) assert.equal(redactBody(raw), redacted, raw);
  for (const keep of RULES.vectors.keep) assert.equal(redactBody(keep), keep);
});

test('a reply keeps its exact bytes apart from sensitive values', () => {
  const body = '{"code":"50111", "data":[{"ordId":"612938300000000001","sCode":"0"}],"price":12345678901234567890}';
  assert.equal(
    redactBody(body),
    `{"code":"50111", "data":[{"ordId":"${REDACTED}","sCode":"0"}],"price":12345678901234567890}`,
  );
  assert.equal(redactBody('<html>502 Bad Gateway</html>'), '<html>502 Bad Gateway</html>');
  assert.equal(redactBody('Your IP 203.0.113.7.'), `Your IP ${REDACTED}.`);
  assert.ok(isSensitive('orderLinkId') && isSensitive('sub_account_id') && !isSensitive('sCode'));
});

test('endpoints drop the host and redact sensitive query values', () => {
  assert.equal(
    redactEndpoint('https://api.venue.test/v1/order?symbol=BTCUSDT&signature=bogus-s&apiKey=bogus-1'),
    `/v1/order?symbol=BTCUSDT&signature=${REDACTED}&apiKey=${REDACTED}`,
  );
  assert.equal(redactEndpoint('https://api.venue.test/v1/time'), '/v1/time');
});

test('cases outside ruling R4 are refused before any request', () => {
  const ok = { name: 'bogus-key', url: 'https://api.venue.test/v1/account', headers: { 'X-API-KEY': 'bogus-key' }, expect };
  assert.equal(checkCases([ok]).length, 1);
  assert.throws(() => checkCases([]), /no capture cases/);
  assert.throws(() => checkCases([{ ...ok, headers: { 'X-API-KEY': 'live-key-material' } }]), /header X-API-KEY/);
  assert.throws(() => checkCases([{ ...ok, headers: { Cookie: 'session=abc' } }]), /header Cookie/);
  assert.throws(() => checkCases([{ ...ok, headers: { 'X-Session': 'abc' } }]), /header X-Session/);
  assert.equal(checkCases([{ ...ok, headers: { Authorization: 'Bearer bogus-jwt' } }]).length, 1);
  assert.throws(() => checkCases([{ ...ok, url: 'https://api.venue.test/v1/account?apiKey=live&signature=bogus-s' }]), /query apiKey/);
  assert.throws(() => checkCases([{ ...ok, url: 'https://api.venue.test/v1/account?signature=3f9a' }]), /query signature/);
  assert.equal(checkCases([{ ...ok, url: 'https://api.venue.test/v1/account?timestamp=1&signature=bogus-s' }]).length, 1);
  assert.throws(() => checkCases([{ ...ok, body: { login: { username: 'trader', password: 'hunter2' } } }]), /body password/);
  assert.throws(() => checkCases([{ ...ok, body: [{ args: [{ apiKey: 'live' }] }] }]), /body apiKey/);
  assert.throws(() => checkCases([{ ...ok, body: { apiKey: ['live'] } }]), /body apiKey/);
  assert.throws(() => checkCases([{ ...ok, body: { vaultAddress: '0xabc' } }]), /body vaultAddress/);
  assert.equal(checkCases([{ ...ok, body: { op: 'login', args: [{ apiKey: 'bogus-k', sign: '' }] } }]).length, 1);
  assert.throws(() => checkCases([{ ...ok, url: 'http://api.venue.test/' }]), /https only/);
  assert.throws(() => checkCases([ok, ok]), /duplicate/);
  assert.throws(() => checkCases([{ ...ok, expect: {} }]), /expect.path/);
  const many = (host) => Array.from({ length: MAX_PER_HOST + 1 }, (_, i) => ({ ...ok, name: `case-${i}`, url: `https://${host(i)}/v1` }));
  assert.throws(() => checkCases(many(() => 'api.venue.test')), /per host/);
  assert.equal(checkCases(many((i) => (i % 2 ? 'a.venue.test' : 'b.venue.test'))).length, MAX_PER_HOST + 1);
});

test('a fixture carries live provenance in the shape the Rust loader reads', () => {
  const item = { name: 'x', url: 'https://api.venue.test/v1/order?orderId=7', expect };
  assert.deepEqual(fixture(item, 401, '{"code":-2015}', new Date('2026-10-28T02:00:00Z')), {
    source: 'live',
    captured_at: '2026-10-28T02:00:00.000Z',
    endpoint: `/v1/order?orderId=${REDACTED}`,
    http_status: 401,
    body: '{"code":-2015}',
    expect,
  });
});

test('capture paces its requests, never follows a redirect and writes one fixture per case', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'error-capture-'));
  const seen = [];
  const fetch = async (url, init) => {
    seen.push({ url, init });
    return { status: 400, text: async () => '{"code":-1121,"msg":"Invalid symbol."}' };
  };
  try {
    const cases = [
      { name: 'invalid-symbol', url: 'https://api.venue.test/v1/depth?symbol=NOPE', expect: { ...expect, kind: 'invalidInput', path: 'read' } },
      { name: 'bogus-key', method: 'GET', url: 'https://api.venue.test/v1/account', headers: { 'X-MBX-APIKEY': 'bogus-key' }, expect },
    ];
    const started = Date.now();
    const written = await capture(cases, dir, { fetch, pace: 40 });
    assert.ok(Date.now() - started >= 35, 'the second request waits the pace');
    assert.deepEqual(written.map((path) => path.slice(dir.length + 1)), ['invalid-symbol.json', 'bogus-key.json']);
    assert.equal(seen.length, 2);
    assert.equal(seen[1].init.headers['X-MBX-APIKEY'], 'bogus-key');
    assert.equal(seen[0].init.redirect, 'manual');
    assert.ok(seen[0].init.signal instanceof AbortSignal);
    const saved = JSON.parse(readFileSync(written[0], 'utf8'));
    assert.equal(saved.endpoint, '/v1/depth?symbol=NOPE');
    assert.equal(saved.body, '{"code":-1121,"msg":"Invalid symbol."}');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
