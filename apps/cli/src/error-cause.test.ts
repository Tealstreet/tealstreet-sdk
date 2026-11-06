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
    ['an undefined body', undefined, 'bybitdemo error (no details: undefined)'],
  ])('names the cause of %s', (_, body, expected) => {
    expect(terminalErrorText(['bybitdemo error', body])).toBe(expected);
  });

  it('keeps the messages it already printed', () => {
    expect(terminalErrorText(['Engine frame apply failed', new Error('bad section')])).toBe(
      'Engine frame apply failed bad section',
    );
    expect(terminalErrorText([{ retMsg: 'API key is invalid.' }])).toBe('API key is invalid.');
    expect(terminalErrorText([{ message: 'boom', code: 'E1' }])).toBe('boom');
    expect(terminalErrorText(['Exchange error:', 'socket closed'])).toBe('socket closed');
  });

  it('prints nothing only for blank or redundant strings', () => {
    expect(terminalErrorText([])).toBeNull();
    expect(terminalErrorText([''])).toBeNull();
    expect(terminalErrorText(['exchange error'])).toBeNull();
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
