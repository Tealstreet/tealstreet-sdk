import { describe, expect, it } from 'vitest';

import { scrub, scrubString } from './scrub';

/**
 * This scrubber is the only thing between a venue error message and Sentry, and
 * the services it protects hold users' exchange credentials. It moved here from
 * `apps/cli` so there is exactly one copy; these tests are what stops the copy
 * being weakened by someone who cannot see all three call sites.
 */
describe('scrubString', () => {
  it('redacts an API-key-shaped run of characters', () => {
    const key = 'AKIA1234567890ABCDEFGHIJKLMNOPQRSTUV';
    expect(scrubString(`auth failed for ${key}`)).toBe('auth failed for [redacted]');
  });

  it('redacts a JWT segment', () => {
    const jwt = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9';
    expect(scrubString(jwt)).toBe('[redacted]');
  });

  it('leaves a FILE PATH alone — a stack trace is mostly paths', () => {
    // The pattern used to include `/`, so this whole path matched as one
    // key-shaped run and Sentry received `/[redacted].ts:62:20` as the entire
    // stack of a signing timeout.
    const frame = '/app/services/exchange-server/src/remote-signer.ts:62:20';
    expect(scrubString(frame)).toBe(frame);
    expect(scrubString('at sign (/Users/alice/src/project/packages/safe-cex/src/exchanges/binance/binance.api.ts:118:9)')).toContain(
      'binance.api.ts:118:9',
    );
  });

  it('still redacts a signature inside a URL, which is the part that matters', () => {
    const url = 'https://api.binance.com/api/v3/order?symbol=BTCUSDT&signature=c5f0a1b2d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f60718293a4b5c6d7e8';
    const scrubbed = scrubString(url);
    expect(scrubbed).toContain('https://api.binance.com/api/v3/order');
    expect(scrubbed).toContain('[redacted]');
    expect(scrubbed).not.toContain('c5f0a1b2d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f60718293a4b5c6d7e8');
  });

  it('redacts base64 that contains a slash, which the main rule walks past', () => {
    const secret = 'aB3/dEf5GhI7jKl9MnO1pQr3StU5vWx7Yz9AbC1dEf3G';
    expect(scrubString(`key=${secret}`)).toBe('key=[redacted]');
  });

  it('leaves ordinary prose alone', () => {
    const message = 'Way too many requests; IP(35.75.147.201) banned until 1788849295904.';
    expect(scrubString(message)).toBe(message);
  });
});

describe('scrub', () => {
  it('redacts by key name whatever the value looks like', () => {
    expect(scrub({ apiKey: 'short', authorization: 'Bearer x', proxyToken: 'y' })).toEqual({
      apiKey: '[redacted]',
      authorization: '[redacted]',
      proxyToken: '[redacted]',
    });
  });

  it('covers the key spellings that actually appear in these services', () => {
    for (const name of ['api_key', 'API-KEY', 'apiSecret', 'passphrase', 'privateKey', 'proxykey', 'seed']) {
      expect(scrub({ [name]: 'value' })).toEqual({ [name]: '[redacted]' });
    }
  });

  it('recurses into nested objects and arrays', () => {
    expect(scrub({ account: { secret: 'x', name: 'ok' }, list: [{ token: 'y' }] })).toEqual({
      account: { secret: '[redacted]', name: 'ok' },
      list: [{ token: '[redacted]' }],
    });
  });

  it('stops at depth 4 rather than recursing forever', () => {
    const deep = { a: { b: { c: { d: { e: 'too far' } } } } };
    expect(JSON.stringify(scrub(deep))).toContain('[truncated]');
  });

  it('caps array and object width', () => {
    expect((scrub(Array.from({ length: 40 }, (_, i) => i)) as unknown[]).length).toBe(16);
    expect(
      Object.keys(scrub(Object.fromEntries(Array.from({ length: 50 }, (_, i) => [`k${i}`, i]))) as object),
    ).toHaveLength(32);
  });

  it('does not choke on primitives', () => {
    expect(scrub(null)).toBe(null);
    expect(scrub(undefined)).toBe(undefined);
    expect(scrub(42)).toBe(42);
  });
});
