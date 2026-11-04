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
