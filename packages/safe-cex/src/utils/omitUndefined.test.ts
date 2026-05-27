import { describe, it, expect } from 'vitest';
import { omitUndefined } from './omitUndefined';

describe('omitUndefined', () => {
  it('should remove undefined values', () => {
    const obj = { a: 1, b: undefined, c: 'string' };
    const result = omitUndefined(obj);

    expect(result).toEqual({ a: 1, c: 'string' });
    expect(result).not.toHaveProperty('b');
  });

  it('should keep null values', () => {
    const obj = { a: null, b: undefined };
    const result = omitUndefined(obj);

    expect(result).toEqual({ a: null });
    expect(result).toHaveProperty('a');
    expect(result).not.toHaveProperty('b');
  });

  it('should keep falsy values (except undefined)', () => {
    const obj = { a: 0, b: '', c: false, d: undefined };
    const result = omitUndefined(obj);

    expect(result).toEqual({ a: 0, b: '', c: false });
  });

  it('should handle empty object', () => {
    const obj = {};
    const result = omitUndefined(obj);

    expect(result).toEqual({});
  });

  it('should handle object with no undefined values', () => {
    const obj = { a: 1, b: 2, c: 3 };
    const result = omitUndefined(obj);

    expect(result).toEqual(obj);
  });

  it('should handle object with all undefined values', () => {
    const obj = { a: undefined, b: undefined };
    const result = omitUndefined(obj);

    expect(result).toEqual({});
  });

  it('should not remove undefined in nested objects', () => {
    const obj = { a: 1, nested: { b: undefined } };
    const result = omitUndefined(obj);

    // omitUndefined only removes top-level undefined
    expect(result.nested).toHaveProperty('b');
    expect(result.nested.b).toBeUndefined();
  });
});
