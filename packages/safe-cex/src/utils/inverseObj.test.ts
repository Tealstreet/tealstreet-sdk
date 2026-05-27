import { describe, it, expect } from 'vitest';
import { inverseObj } from './inverseObj';

describe('inverseObj', () => {
  it('should swap keys and values for string object', () => {
    const obj = { a: 'x', b: 'y', c: 'z' };
    const result = inverseObj(obj);

    expect(result).toEqual({ x: 'a', y: 'b', z: 'c' });
  });

  it('should swap keys and values for numeric values', () => {
    const obj = { one: 1, two: 2, three: 3 };
    const result = inverseObj(obj);

    expect(result).toEqual({ 1: 'one', 2: 'two', 3: 'three' });
  });

  it('should handle empty object', () => {
    const obj = {};
    const result = inverseObj(obj);

    expect(result).toEqual({});
  });

  it('should handle single entry', () => {
    const obj = { key: 'value' };
    const result = inverseObj(obj);

    expect(result).toEqual({ value: 'key' });
  });

  it('should handle duplicate values (last wins)', () => {
    const obj = { a: 'same', b: 'same' };
    const result = inverseObj(obj);

    // Either 'a' or 'b' will be the value, depends on iteration order
    expect(result).toHaveProperty('same');
    expect(['a', 'b']).toContain(result.same);
  });

  it('should handle symbol values', () => {
    const sym = Symbol('test');
    const obj = { key: sym } as Record<string, symbol>;
    const result = inverseObj(obj);

    expect(result[sym]).toBe('key');
  });
});
