import { describe, expect, it } from 'vitest';

import { connectFailureCode, findKeyRejection, stripAnsi, summarizeOneShot } from './oneShotResult';

describe('summarizeOneShot', () => {
  it('is OK without error lines, and failed on the first error line or a throw', () => {
    expect(summarizeOneShot([{ kind: 'success', text: 'Order placed' }])).toEqual({ ok: true, code: 'OK' });
    expect(
      summarizeOneShot([
        { kind: 'warn', text: 'slow' },
        { kind: 'error', text: 'USD or % required' },
      ]),
    ).toEqual({
      ok: false,
      code: 'COMMAND_FAILED',
      error: 'USD or % required',
    });
    expect(summarizeOneShot([], new Error('boom'))).toMatchObject({ ok: false, error: 'boom' });
    expect(summarizeOneShot([{ kind: 'error', text: 'USD or % required' }], undefined, [])).toMatchObject({
      code: 'COMMAND_FAILED',
    });
    // The command printed success but the exchange logged an error: a venue that
    // rejected and only logged it, or one that logged and then handled it.
    expect(
      summarizeOneShot([{ kind: 'success', text: 'Order placed' }], undefined, [
        { kind: 'error', text: 'Exchange error: insufficient margin' },
      ]),
    ).toMatchObject({ ok: false, code: 'UNCONFIRMED', error: 'Exchange error: insufficient margin' });
    expect(
      summarizeOneShot([{ kind: 'success', text: 'Order placed' }], undefined, [{ kind: 'warn', text: 'slow' }]),
    ).toEqual({ ok: true, code: 'OK' });
  });
});

describe('connectFailureCode', () => {
  it('splits a sign-in failure by whether the user ever signed in', () => {
    const line = 'Failed to connect: Authentication failed (401)';
    expect(connectFailureCode(line, { state: 'not_logged_in' })).toBe('NOT_LOGGED_IN');
    expect(connectFailureCode(line, { state: 'expired' })).toBe('AUTH_EXPIRED');
  });

  it('names timeouts and falls back to a generic failure', () => {
    expect(connectFailureCode('Failed to connect: request timed out', { state: 'signed_in' })).toBe(
      'CONNECTION_TIMEOUT',
    );
    expect(connectFailureCode('Failed to connect: ECONNRESET', { state: 'signed_in' })).toBe('CONNECTION_FAILED');
    expect(connectFailureCode(undefined, { state: 'signed_in' })).toBe('CONNECTION_FAILED');
  });
});

describe('stripAnsi', () => {
  it('removes colour codes', () => {
    expect(stripAnsi('\u001b[31mfailed\u001b[0m')).toBe('failed');
  });
});

describe('findKeyRejection', () => {
  it('spots a venue refusing the key, including Bybit logging it on an HTTP 200', () => {
    expect(findKeyRejection(['Loaded 896 Bybit markets', 'API key is invalid.'])).toBe('API key is invalid.');
    expect(
      findKeyRejection(['Bybit market data is ready, but the private API was rejected (x) — trading is unavailable']),
    ).toBeDefined();
    expect(findKeyRejection(['Invalid signature for request'])).toBeDefined();
    expect(findKeyRejection(['OKX: Invalid OK-ACCESS-KEY'])).toBeDefined();
    expect(findKeyRejection(["OKX: API key doesn't exist"])).toBeDefined();
    expect(findKeyRejection(['Binance: Signature for this request is not valid.'])).toBeDefined();
    expect(findKeyRejection(['bybit credentials were rejected'])).toBeDefined();
  });

  it('ignores ordinary exchange chatter', () => {
    expect(
      findKeyRejection(['Loaded 896 Bybit markets', 'Ready to trade on Bybit', 'rate limited, retrying']),
    ).toBeUndefined();
  });
});
