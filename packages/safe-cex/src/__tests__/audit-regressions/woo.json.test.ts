import { describe, expect, it } from 'vitest';
import { parseWooJson } from '../reference/woo/woo.json';

describe('WOO exact integer response parsing', () => {
  it('preserves bare unsafe positive and negative IDs without rounding', () => {
    expect(parseWooJson('{"order_id":9007199254740993,"rows":[18446744073709551615,-9007199254740992]}')).toEqual({
      order_id: '9007199254740993', rows: ['18446744073709551615', '-9007199254740992'],
    });
  });

  it('keeps large numbers inside error messages and escaped strings unchanged', () => {
    const row = {
      success: false,
      message: 'Invalid order_id:9007199254740993 is not found; [18446744073709551615]',
      escaped: 'quote " then \\ and order:9007199254740993 after it',
      orderId: '9007199254740993',
    };
    expect(parseWooJson(JSON.stringify(row))).toEqual(row);
    const withBareId = JSON.stringify(row).replace('"orderId":"9007199254740993"', '"orderId":9007199254740993');
    expect(parseWooJson(withBareId)).toEqual(row);
  });

  it('retains safe integers, decimals and exponent numbers as numbers', () => {
    expect(parseWooJson('[9007199254740991,-9007199254740991,0,-0,1234567890123456,9007199254740993.5,1e16,-2E+16,3e-3]')).toEqual([
      9007199254740991, -9007199254740991, 0, -0, 1234567890123456, 9007199254740993.5, 1e16, -2e16, 0.003,
    ]);
  });

  it('does not repair malformed JSON or log the response', () => {
    for (const raw of ['{"id":09007199254740993}', '[9007199254740993,]', '{"text":"unterminated', '{"n":-}', '{"n":1e}']) {
      expect(parseWooJson(raw)).toBeNull();
    }
  });

  it('passes already decoded Axios responses through', () => {
    const body = { success: true, data: [] };
    expect(parseWooJson(body)).toBe(body);
  });
});
