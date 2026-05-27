import { describe, expect, it } from 'vitest';

import { normalizeReturnForCapture } from './captureWriteback';

describe('normalizeReturnForCapture — SPEC §7.4 table', () => {
  it('returns null for undefined (no capture writeback)', () => {
    expect(normalizeReturnForCapture(undefined)).toBeNull();
  });

  it('returns null for null', () => {
    expect(normalizeReturnForCapture(null)).toBeNull();
  });

  it('passes a string through as-is, no sibling .json', () => {
    expect(normalizeReturnForCapture('order-id-123')).toEqual({ primary: 'order-id-123' });
  });

  it('passes the empty string through (not the same as null)', () => {
    expect(normalizeReturnForCapture('')).toEqual({ primary: '' });
  });

  it('joins a string array on comma, no sibling .json', () => {
    expect(normalizeReturnForCapture(['a', 'b', 'c'])).toEqual({ primary: 'a,b,c' });
  });

  it('comma-joins single-element string arrays', () => {
    expect(normalizeReturnForCapture(['only-one'])).toEqual({ primary: 'only-one' });
  });

  it('treats empty string array as empty primary', () => {
    expect(normalizeReturnForCapture([])).toEqual({ primary: '' });
  });

  it('stringifies numbers, no sibling .json', () => {
    expect(normalizeReturnForCapture(42)).toEqual({ primary: '42' });
    expect(normalizeReturnForCapture(-1.5)).toEqual({ primary: '-1.5' });
    expect(normalizeReturnForCapture(0)).toEqual({ primary: '0' });
  });

  it('stringifies booleans, no sibling .json', () => {
    expect(normalizeReturnForCapture(true)).toEqual({ primary: 'true' });
    expect(normalizeReturnForCapture(false)).toEqual({ primary: 'false' });
  });

  it('JSON-stringifies a plain object AND sets sibling .json', () => {
    const result = normalizeReturnForCapture({ orderId: 'abc', filled: 1 });
    expect(result).toEqual({
      primary: '{"orderId":"abc","filled":1}',
      json: '{"orderId":"abc","filled":1}',
    });
  });

  it('JSON-stringifies mixed-type arrays AND sets sibling .json', () => {
    const result = normalizeReturnForCapture(['a', 1, true]);
    expect(result).toEqual({ primary: '["a",1,true]', json: '["a",1,true]' });
  });

  it('JSON-stringifies nested arrays AND sets sibling .json', () => {
    const result = normalizeReturnForCapture([['a', 'b'], ['c']]);
    expect(result).toEqual({ primary: '[["a","b"],["c"]]', json: '[["a","b"],["c"]]' });
  });

  it('handles cyclic references by emitting the sentinel <unserializable>', () => {
    const cyclic: { self?: unknown } = {};
    cyclic.self = cyclic;
    const result = normalizeReturnForCapture(cyclic);
    expect(result).toEqual({ primary: '<unserializable>' });
  });

  it('preserves the `cancel @x` grammar interop: order-id array round-trips', () => {
    // Real-world: a script that placed two orders returns their IDs as a
    // string array. The host writes `'o1,o2'` to `@<captureTo>`; the next
    // chain step `cancel @<captureTo>` then sees `o1,o2` and splits on comma.
    const result = normalizeReturnForCapture(['order-aaa', 'order-bbb']);
    expect(result?.primary).toBe('order-aaa,order-bbb');
    expect(result?.json).toBeUndefined();
  });

  it('renders Date as JSON (objects with toJSON go through JSON.stringify)', () => {
    const d = new Date('2026-01-01T00:00:00.000Z');
    const result = normalizeReturnForCapture(d);
    expect(result).toEqual({
      primary: '"2026-01-01T00:00:00.000Z"',
      json: '"2026-01-01T00:00:00.000Z"',
    });
  });
});
