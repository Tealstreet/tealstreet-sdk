import { describe, expect, it } from 'vitest';

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
