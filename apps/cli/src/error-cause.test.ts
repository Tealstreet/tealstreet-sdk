import { describe, expect, it } from 'vitest';
import { describeCause, terminalErrorText } from './error-cause';

describe('terminalErrorText', () => {
  // 0.10.17's Bybit interceptor called console.error(`${exchange.name} error`, response.data)
  // for any failed envelope; the terminal printed only the prefix for these bodies.
  it.each<[string, unknown, string]>([
    [
      'a proxied raw_response string',
      { raw_response: '{"retCode":10006,"retMsg":"Too many visits!"}' },
      'bybitdemo error Too many visits!',
    ],
    [
      'a snake-case envelope',
      { ret_code: 10002, ret_msg: 'invalid request, please check your server timestamp' },
      'bybitdemo error invalid request, please check your server timestamp',
    ],
    ['a code without a message', { retCode: 110001, retMsg: '' }, 'bybitdemo error error code 110001'],
    ['an unrecognised body', { result: null, time: 1 }, 'bybitdemo error {"result":null,"time":1}'],
    ['an empty object', {}, 'bybitdemo error (empty object)'],
    // A primitive after a label is not described (it was never printed); see the colon rule below.
    ['an undefined body', undefined, 'bybitdemo error'],
  ])('names the cause of %s', (_, body, expected) => {
    expect(terminalErrorText(['bybitdemo error', body])).toBe(expected);
  });

  it('keeps every argument that already printed something byte-identical', () => {
    expect(terminalErrorText(['Engine frame apply failed', new Error('bad section')])).toBe(
      'Engine frame apply failed bad section',
    );
    expect(terminalErrorText(['x', new Error('  padded message ')])).toBe('x   padded message ');
    expect(terminalErrorText([{ retMsg: 'API key is invalid.' }])).toBe('API key is invalid.');
    expect(terminalErrorText([{ message: 'boom', code: 'E1' }])).toBe('boom');
    expect(terminalErrorText([{ message: 404 }, { msg: null }])).toBe('404 null');
    expect(terminalErrorText(['Exchange error:', 'socket closed'])).toBe('socket closed');
  });

  it('adds a trailing primitive only to a bare or colon-ended prefix', () => {
    expect(terminalErrorText(['Retrying in', 3, 'seconds'])).toBe('Retrying in seconds');
    expect(terminalErrorText(['Order failed', undefined])).toBe('Order failed');
    expect(terminalErrorText(['Order failed', null, false, ['a']])).toBe('Order failed');
    expect(terminalErrorText(['Order failed:', undefined])).toBe('Order failed: (no details: undefined)');
    expect(terminalErrorText([404])).toBe('404');
  });

  it('never prints credentials from a serialized body', () => {
    const text = terminalErrorText([
      'bybitdemo error',
      {
        url: '/v5/order/create',
        headers: { 'X-BAPI-API-KEY': 'live-key', 'X-BAPI-SIGN': 'deadbeef', 'X-BAPI-TIMESTAMP': '1' },
        config: {
          secret: 's3cret',
          passphrase: 'p',
          token: 't',
          cookie: 'c',
          privateKey: 'k',
          seed: 'w',
          mnemonic: 'm',
        },
      },
    ]);
    for (const leaked of ['live-key', 'deadbeef', 's3cret', '"p"', '"t"', '"c"', '"k"', '"w"', '"m"']) {
      expect(text).not.toContain(leaked);
    }
    expect(text).toContain('"X-BAPI-API-KEY":"[redacted]"');
    expect(text).toContain('"X-BAPI-TIMESTAMP":"1"');
  });

  it('masks credentials inside a JSON string body and under jwt/credential keys', () => {
    const text = terminalErrorText([
      'bybitdemo error',
      {
        raw_response: JSON.stringify({
          headers: { 'X-BAPI-API-KEY': 'live-key', 'X-BAPI-SIGN': 'deadbeef', 'X-BAPI-TIMESTAMP': '1' },
        }),
        jwt: 'eyJhbGciOi',
        credentials: { id: 'c1' },
      },
    ]);
    for (const leaked of ['live-key', 'deadbeef', 'eyJhbGciOi', 'c1']) expect(text).not.toContain(leaked);
    expect(text).toContain('"X-BAPI-API-KEY":"[redacted]"');
    expect(text).toContain('"X-BAPI-SIGN":"[redacted]"');
    expect(text).toContain('"X-BAPI-TIMESTAMP":"1"');
  });

  it('prints nothing for blank or redundant strings', () => {
    expect(terminalErrorText([])).toBeNull();
    expect(terminalErrorText([''])).toBe('');
    expect(terminalErrorText(['exchange error'])).toBe('');
    expect(terminalErrorText(['exchange error', {}])).toBe('(empty object)');
  });
});

describe('describeCause', () => {
  it('reads nested envelopes, error causes and HTTP responses', () => {
    expect(describeCause({ data: { retCode: 10001, retMsg: 'params error' } })).toBe('params error');
    expect(describeCause(new Error('', { cause: { kind: 'Timeout' } }))).toBe('error code Timeout');
    expect(describeCause(Object.assign(new Error(''), { response: { data: { msg: 'Forbidden' } } }))).toBe('Forbidden');
  });

  it('is never empty', () => {
    expect(describeCause(new Error(''))).toBe('Error (no message)');
    expect(describeCause(Object.assign(new Error(''), { name: 'TimeoutError' }))).toBe('TimeoutError');
    expect(describeCause(null)).toBe('(no details: null)');
    expect(describeCause('')).toBe('(empty string)');
    expect(describeCause(0)).toBe('0');
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(describeCause(circular)).toBe('(unserializable object)');
  });

  it('bounds a serialized body', () => {
    const cause = describeCause({ blob: 'x'.repeat(1_000) });
    expect(cause.length).toBe(300);
    expect(cause.endsWith('...')).toBe(true);
  });
});
