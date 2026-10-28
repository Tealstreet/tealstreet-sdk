import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { MAX_CASES, REDACTED, capture, checkCases, fixture, redactBody, redactEndpoint } from './error-capture.mjs';

const expect = { kind: 'authInvalid', outcome: 'rejected', path: 'startup' };

test('a reply keeps its exact bytes apart from sensitive values', () => {
  const body = '{"code":"50111","msg":"Invalid OK-ACCESS-KEY","data":[{"ordId":"612938300000000001","sCode":"0", "price":12345678901234567890}]}';
  assert.equal(
    redactBody(body),
    `{"code":"50111","msg":"Invalid OK-ACCESS-KEY","data":[{"ordId":"${REDACTED}","sCode":"0", "price":12345678901234567890}]}`,
  );
  assert.equal(redactBody('{"orderId": 4294967295,"uid":""}'), `{"orderId": "${REDACTED}","uid":""}`);
  assert.equal(redactBody('<html>502 Bad Gateway</html>'), '<html>502 Bad Gateway</html>');
  assert.equal(redactBody(`sig ${'ab'.repeat(32)} rejected`), `sig ${REDACTED} rejected`);
});

test('endpoints drop the host and redact sensitive query values', () => {
  assert.equal(
    redactEndpoint('https://api.venue.test/v1/order?symbol=BTCUSDT&signature=abc&apiKey=bogus-1'),
    `/v1/order?symbol=BTCUSDT&signature=${REDACTED}&apiKey=${REDACTED}`,
  );
  assert.equal(redactEndpoint('https://api.venue.test/v1/time'), '/v1/time');
});

test('cases outside ruling R4 are refused before any request', () => {
  const ok = { name: 'bogus-key', url: 'https://api.venue.test/v1/account', headers: { 'X-API-KEY': 'bogus-key' }, expect };
  assert.equal(checkCases([ok]).length, 1);
  assert.throws(() => checkCases([]), /no capture cases/);
  assert.throws(() => checkCases([{ ...ok, headers: { 'X-API-KEY': 'live-key-material' } }]), /bogus-/);
  assert.throws(() => checkCases([{ ...ok, url: 'http://api.venue.test/' }]), /https only/);
  assert.throws(() => checkCases([ok, ok]), /duplicate/);
  assert.throws(() => checkCases([{ ...ok, expect: {} }]), /expect.path/);
  const many = Array.from({ length: MAX_CASES + 1 }, (_, i) => ({ ...ok, name: `case-${i}` }));
  assert.throws(() => checkCases(many), /at most/);
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

test('capture paces its requests and writes one fixture per case', async () => {
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
    const saved = JSON.parse(readFileSync(written[0], 'utf8'));
    assert.equal(saved.endpoint, '/v1/depth?symbol=NOPE');
    assert.equal(saved.body, '{"code":-1121,"msg":"Invalid symbol."}');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
