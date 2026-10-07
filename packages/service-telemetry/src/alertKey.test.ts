import { describe, expect, it } from 'vitest';

import { alertKey } from './alertKey';

/**
 * The throttle bucket IS the key, so anything unique per occurrence disables
 * throttling silently. Every pair below produced two distinct keys before
 * normalization, and so posted twice inside one 15-minute window.
 */
describe('alertKey collapses volatile tokens', () => {
  const same = (a: string, b: string) =>
    expect(alertKey('unhandled-rejection', new Error(a))).toBe(alertKey('unhandled-rejection', new Error(b)));

  it('collapses nanoid request ids', () => {
    same(
      '[binance] Request setup error: Signing request Uj1n1kabvdORzqSyYHyV8 timed out after 15000ms',
      '[binance] Request setup error: Signing request DGr_PH7-PeWaWx1eUpa5D timed out after 15000ms',
    );
  });

  it('collapses IPs and epoch timestamps', () => {
    same(
      'Way too many requests; IP(35.75.147.201) banned until 1788849295904.',
      'Way too many requests; IP(54.238.24.246) banned until 1788852945364.',
    );
  });

  const distinct = (a: string, b: string) =>
    expect(alertKey('unhandled-rejection', new Error(a))).not.toBe(alertKey('unhandled-rejection', new Error(b)));

  it('keeps genuinely different conditions apart', () => {
    distinct('Remote signer unavailable', 'Invalid API-key');
  });

  /**
   * Over-collapsing is the worse failure of the two: a bucket shared with an
   * unrelated condition silences a real alert for the whole window. An earlier
   * cut of this normalized every bare number and every 12-char word.
   */
  it('does not collapse two different HTTP status codes', () => {
    distinct('Request failed with status code 401', 'Request failed with status code 503');
  });

  it('does not collapse two different Redis ports', () => {
    distinct('Redis connection to 10.4.1.7:6379 failed', 'Redis connection to 10.4.1.7:6380 failed');
  });

  it('leaves ordinary long words alone', () => {
    // `disconnected`, `ECONNREFUSED` and `shared-public-restart` are all long
    // enough to have been eaten by a length-only rule.
    for (const word of ['disconnected', 'ECONNREFUSED', 'shared-public-restart', 'ReferenceError']) {
      expect(alertKey('unhandled-rejection', new Error(word))).toContain(word);
    }
  });

  it('does not collapse two distinct error names', () => {
    distinct('ReferenceError: foo is not defined', 'ReferenceError: bar is not defined');
  });

  it('keeps the prefix, so severity classification still works', () => {
    expect(alertKey('uncaught-exception', new Error('boom'))).toMatch(/^uncaught-exception:/);
  });

  /**
   * A throw here re-enters as an uncaughtException — the one key that still
   * pages — so a malformed rejection would alert the critical channel for
   * nothing. `Promise.reject()` with no argument is a real shape.
   */
  it('survives values JSON.stringify cannot handle', () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;

    for (const value of [undefined, null, Symbol('x'), 1n, circular]) {
      expect(() => alertKey('unhandled-rejection', value)).not.toThrow();
      expect(alertKey('unhandled-rejection', value)).toMatch(/^unhandled-rejection:/);
    }
  });

  it('always emits the colon the per-session scoping test depends on', () => {
    // `PROCESS_HANDLER_KEYS` requires it; an empty signature must not drop it.
    expect(alertKey('unhandled-rejection', {})).toMatch(/^unhandled-rejection:/);
  });
});
