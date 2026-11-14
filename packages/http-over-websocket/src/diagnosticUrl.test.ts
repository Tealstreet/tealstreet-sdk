import { describe, expect, it, test } from 'vitest';

import { diagnosticUrl, stripDiagnosticUrlQueries } from './diagnosticUrl';
import { parseRelayUrl } from './tunnelEndpoints';

describe('diagnostic URLs', () => {
  it.each([
    ['https://user:password@api.example.com/private?signature=secret#token', 'https://api.example.com/private'],
    ['/private?sign=secret#token', '/private'],
    ['not-a-url?key=secret', '[invalid URL]'],
  ])('keeps only origin and path for %s', (url, expected) => {
    expect(diagnosticUrl(url)).toBe(expected);
  });

  it('strips every embedded query without losing the surrounding failure context', () => {
    expect(
      stripDiagnosticUrlQueries(
        'lost (URL: https://api.example.com/a?sign=secret), peer wss://ws.example.com/b?token=secret',
      ),
    ).toBe('lost (URL: https://api.example.com/a), peer wss://ws.example.com/b');
  });

  it('does not expose query strings when rejecting a non-relay URL', () => {
    expect(() => parseRelayUrl('https://api.example.com/private?key=secret')).toThrow(
      'not a relay url: https://api.example.com/private',
    );
    try {
      parseRelayUrl('https://api.example.com/private?key=secret');
    } catch (error) {
      expect((error as Error).message).not.toContain('secret');
    }
  });
});

test.each([
  ['relative', 'GET /private?signature=fixture-secret failed'],
  ['encoded', encodeURIComponent('https://venue.example/private?signature=fixture-secret')],
  ['JSON escaped', String.raw`lost https:\/\/venue.example\/private?signature=fixture-secret`],
])('does not leak %s upstream URL text', (_, text) => {
  expect(stripDiagnosticUrlQueries(text)).not.toContain('fixture-secret');
});

it('fails closed across encoding layers, Unicode keys and encoded delimiters in secrets', () => {
  for (const original of [
    '/private?signature=fixture-secret%26hidden%20tail',
    'token=prefix fixture-secret',
    JSON.stringify({ headers: [['X-API-KEY', 'fixture-secret']] }),
    JSON.stringify({ signature: { value: 'fixture-secret' } }),
    JSON.stringify({ request: JSON.stringify({ sign: 'fixture-secret&hidden tail' }) }),
    String.raw`{"\u0073ignature":"fixture-secret"}`,
  ]) {
    let encoded = original;
    for (let depth = 0; depth <= 6; depth++) {
      expect(stripDiagnosticUrlQueries(encoded)).not.toContain('fixture-secret');
      encoded = encodeURIComponent(encoded);
    }
  }
});

describe('independent new mixed-encoding probes', () => {
  const secret = 'probe-auth-value';
  const raw = `https://venue.example/private?Signature=${secret}&symbol=BTC`;
  const cases: [string, string, string][] = [
    ['mixed percent JSON HTML', encodeURIComponent(JSON.stringify({ url: raw.replace('&', '&amp;') })), secret],
    [
      'JSON escaped unicode',
      JSON.stringify({ error: String.raw`https:\/\/venue.example\/private?\u0053ignature=probe-auth-value` }),
      secret,
    ],
    ['HTML decimal key', `/private?&#83;ignature=${secret}`, secret],
    ['HTML hex key', `/private?&#x53;ignature=${secret}`, secret],
    ['HTML colon and quotes', `error {&quot;passphrase&quot;&#58;&quot;${secret}&quot;}`, secret],
    ['standalone base64 query', btoa(`Signature=${secret}&symbol=BTC`), btoa(`Signature=${secret}&symbol=BTC`)],
    [
      'wrapped base64 query',
      '/private?query=' + btoa(`Signature=${secret}&symbol=BTC`),
      btoa(`Signature=${secret}&symbol=BTC`),
    ],
    ['title-case', `/private?Signature=${secret}`, secret],
    ['header spelling', `/private?X-MBX-APIKEY=${secret}`, secret],
    ['JSON header spelling', JSON.stringify({ 'X-MBX-APIKEY': secret, status: 503 }), secret],
  ];
  test.each(cases)('scrubs %s', (_, text, leaked) => {
    expect(stripDiagnosticUrlQueries(text)).not.toContain(leaked);
  });
});

it('bounds base64 inspection and preserves harmless printable and binary tokens', () => {
  expect(stripDiagnosticUrlQueries('token=&quot;probe-auth-value&quot;')).not.toContain('probe-auth-value');
  expect(stripDiagnosticUrlQueries('/private?token=prefix&#32;probe-auth-value')).not.toContain('probe-auth-value');
  const padded = btoa('key=secret');
  expect(padded).toHaveLength(16);
  expect(stripDiagnosticUrlQueries(padded)).not.toContain(padded);
  expect(stripDiagnosticUrlQueries(btoa('symbol=BTC'))).toBe(btoa('symbol=BTC'));
  const benign = btoa('symbol=BTC&limit=10');
  const binary = btoa('\0'.repeat(16));
  expect(stripDiagnosticUrlQueries(benign)).toBe(benign);
  expect(stripDiagnosticUrlQueries(binary)).toBe(binary);
  const base64url = btoa('Signature=probe-auth-value&x=' + String.fromCharCode(255, 255))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
  expect(stripDiagnosticUrlQueries(base64url)).not.toContain(base64url);
  expect(stripDiagnosticUrlQueries('/private?query=' + base64url)).not.toContain(base64url);
  let nested = btoa('Signature=probe-auth-value&symbol=BTC');
  for (let depth = 0; depth < 6; depth++) {
    expect(stripDiagnosticUrlQueries(nested)).not.toContain(nested);
    nested = btoa(nested);
  }
  const oversized = 'A'.repeat(4097);
  expect(stripDiagnosticUrlQueries(oversized)).not.toContain(oversized);
});

describe('credentials after diagnostic labels', () => {
  it.each(['apiKey', 'token', 'Authorization'])('removes %s material without dropping ordinary context', (key) => {
    const material = 'SYNTHETIC_REVIEW_3436_ONLY';
    for (const prefix of ['request failed: ', 'request failed:', 'connection: request failed: ', 'Error: ']) {
      const safe = stripDiagnosticUrlQueries(`${prefix}${key}="${material}" status=503`);
      expect(safe).not.toContain(material);
      expect(safe).toContain(prefix);
      expect(safe).toContain('status=503');
    }
  });

  it('retains nonsecret assignments after repeated labels', () => {
    const message = 'request failed: peer replied: status=503 symbol=BTCUSDT';
    expect(stripDiagnosticUrlQueries(message)).toBe(message);
  });
});
