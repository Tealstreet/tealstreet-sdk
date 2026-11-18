import { describe, expect, it } from 'vitest';

import { scrub, scrubString } from './scrub';

/**
 * Services and CLI use the same credential policy as every diagnostic sink.
 * Service payload bounds are formatting limits, independent of redaction.
 */
describe('scrubString', () => {
  it('redacts an API-key-shaped run of characters', () => {
    const key = 'AKIA1234567890ABCDEFGHIJKLMNOPQRSTUV';
    expect(scrubString(`auth failed for ${key}`)).toBe('auth failed for [REDACTED]');
  });

  it('redacts a JWT segment', () => {
    const jwt = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9';
    expect(scrubString(jwt)).toBe('[REDACTED]');
  });

  it('scrubs proof-shaped paths while retaining short ordinary frame text', () => {
    // Long proof-shaped paths no longer get a service-specific exemption.
    const frame = '/app/services/exchange-server/src/remote-signer.ts:62:20';
    expect(scrubString(frame)).toBe('[REDACTED]');
    const shortFrame = 'at handler (worker.ts:62:20)';
    expect(scrubString(shortFrame)).toBe(shortFrame);
  });

  it('still redacts a signature inside a URL, which is the part that matters', () => {
    const url = 'https://api.binance.com/api/v3/order?symbol=BTCUSDT&signature=c5f0a1b2d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f60718293a4b5c6d7e8';
    const scrubbed = scrubString(url);
    expect(scrubbed).toContain('https://api.binance.com/api/v3/order');
    expect(scrubbed).toContain('[REDACTED]');
    expect(scrubbed).not.toContain('c5f0a1b2d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f60718293a4b5c6d7e8');
  });

  it('redacts base64 that contains a slash, which the main rule walks past', () => {
    const secret = 'aB3/dEf5GhI7jKl9MnO1pQr3StU5vWx7Yz9AbC1dEf3G';
    expect(scrubString(`key=${secret}`)).toBe('[REDACTED]');
  });

  it('leaves ordinary prose alone', () => {
    const message = 'Way too many requests; IP(35.75.147.201) banned until 1788849295904.';
    expect(scrubString(message)).toBe(message);
  });
});

describe('scrub', () => {
  it('redacts by key name whatever the value looks like', () => {
    expect(scrub({ apiKey: 'short', authorization: 'Bearer x', proxyToken: 'y' })).toEqual({
      apiKey: '[REDACTED]',
      authorization: '[REDACTED]',
      proxyToken: '[REDACTED]',
    });
  });

  it('covers the key spellings that actually appear in these services', () => {
    for (const name of ['api_key', 'API-KEY', 'apiSecret', 'passphrase', 'privateKey', 'proxykey', 'seed']) {
      expect(scrub({ [name]: 'value' })).toEqual({ [name]: '[REDACTED]' });
    }
  });

  it('recurses into nested objects and arrays', () => {
    expect(scrub({ account: { secret: 'x', name: 'ok' }, list: [{ token: 'y' }] })).toEqual({
      account: { secret: '[REDACTED]', name: 'ok' },
      list: [{ token: '[REDACTED]' }],
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
