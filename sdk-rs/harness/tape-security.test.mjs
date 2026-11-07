import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync, readdirSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { createRecordingPolicy } from './polling.mjs';
import { tapeHeaders } from './tape-fields.mjs';

test('recording preserves response protocol headers but never Set-Cookie in any supported header shape', () => {
  const cases = ['content-type: application/json\nSet-Cookie: live-session=secret-value\nx-request-id: 1234',
    { 'content-type': 'application/json', 'set-cookie': 'live-session=secret-value' },
    [['Content-Type', 'application/json'], ['SET-COOKIE', 'live-session=secret-value']],
    new Headers({ 'Content-Type': 'application/json', 'Set-Cookie': 'live-session=secret-value' })];
  for (const headers of cases) {
    const pairs = tapeHeaders(headers);
    assert.ok(pairs.some(([key]) => key.toLowerCase() === 'content-type'));
    assert.ok(!JSON.stringify(pairs).includes('secret-value'));
    const safe = createRecordingPolicy({}, {}).sanitize({ type: 'http', headers, body: '{"price":"123"}' });
    assert.ok(!safe.headers.toLowerCase().includes('set-cookie'));
    assert.equal(safe.body, '{"price":"123"}');
  }
});

test('committed replay fixtures contain no response session cookies', () => {
  const directory = new URL('./fixtures/', import.meta.url);
  for (const file of readdirSync(directory).filter((name) => name.endsWith('.jsonl.gz'))) {
    for (const line of gunzipSync(readFileSync(new URL(file, directory))).toString().split('\n').filter(Boolean)) {
      const record = JSON.parse(line);
      const headers = record.headers;
      const names = typeof headers === 'string' ? headers.split(/\r?\n/).map((line) => line.split(':', 1)[0]) :
        Array.isArray(headers) ? headers.map(([name]) => name) : Object.keys(headers ?? {});
      assert.ok(!names.some((name) => name.trim().toLowerCase() === 'set-cookie'), `${file}: session cookie`);
    }
  }
});

test('generic API key exemptions require the exact public vector and fixture together', () => {
  const policy = readFileSync(new URL('../../.gitleaks.toml', import.meta.url), 'utf8');
  const entries = policy.split('[[rules.allowlists]]').slice(1).map((entry) => {
    assert.match(entry, /condition\s*=\s*"AND"/);
    assert.match(entry, /regexTarget\s*=\s*"secret"/);
    const expressions = (name) => [...entry.match(new RegExp(`${name}\\s*=\\s*\\[([\\s\\S]*?)\\]`))[1]
      .matchAll(/"(?:[^"\\]|\\.)*"/g)].map(([literal]) => new RegExp(JSON.parse(literal)));
    return { paths: expressions('paths'), regexes: expressions('regexes') };
  });
  const exempt = (path, secret) => entries.some((entry) => entry.paths.some((rule) => rule.test(path)) &&
    entry.regexes.some((rule) => rule.test(secret)));
  const publicKey = '1'.repeat(64);
  const publicHash = '99291b039f7ecd9952d7fc8fda105c5ca631b597cfbc5a4de4a7f95d435e4470';
  const vector = 'sdk-rs/venues/paradex/tests/sign-vectors.json';
  const provenance = 'sdk-rs/crypto/tests/fixtures/lighter/venue-signing-preimages.json';
  assert.equal(exempt('sdk-rs/harness/venues.mjs', Buffer.from('replay-secret').toString('base64')), true);
  assert.equal(exempt(vector, publicKey), true);
  assert.equal(exempt(provenance, publicHash), true);
  for (const path of [vector, provenance, 'sdk-rs/crypto/tests/new-credential.rs',
    'sdk-rs/engine/tests/secret.rs', 'sdk-rs/venues/binance/src/sign.rs', 'sdk-rs/harness/venues.mjs', 'sdk-rs/venues/paradex/tests/sign-vectors.json.bak'])
    assert.equal(exempt(path, 'new-live-api-key-not-a-public-vector'), false, path);
  assert.equal(exempt('sdk-rs/crypto/tests/new-credential.rs', publicHash), false);
  assert.equal(exempt(vector, publicHash), false);
});
