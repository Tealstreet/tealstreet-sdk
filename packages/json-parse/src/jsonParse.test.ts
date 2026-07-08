import { describe, expect, it } from 'vitest';

import { bigIntAsStringJsonParse } from './jsonParse';

describe('bigIntAsStringJsonParse', () => {
  it('preserves positive 16+ digit integers as strings (no precision loss)', () => {
    const result = bigIntAsStringJsonParse<{ id: string }>('{"id":12345678901234567}');
    expect(result).not.toBeNull();
    expect(result?.id).toBe('12345678901234567');
  });

  it('preserves negative 16+ digit integers as strings with the sign (no precision loss)', () => {
    const result = bigIntAsStringJsonParse<{ id: string }>('{"id":-12345678901234567}');
    expect(result).not.toBeNull();
    expect(result?.id).toBe('-12345678901234567');
  });

  it('leaves 16+ digit numbers with a fractional part untouched and still parses', () => {
    const result = bigIntAsStringJsonParse<{ ts: number }>('{"ts":12345678901234567.5}');
    // Must not become invalid JSON (which would parse to null and drop the message)
    expect(result).not.toBeNull();
    expect(typeof result?.ts).toBe('number');
  });

  it('leaves 16+ digit numbers with an exponent part untouched and still parses', () => {
    const result = bigIntAsStringJsonParse<{ ts: number }>('{"ts":1234567890123456e2}');
    expect(result).not.toBeNull();
    expect(typeof result?.ts).toBe('number');
  });

  it('handles big integers as array elements', () => {
    const result = bigIntAsStringJsonParse<{ ids: string[] }>('{"ids":[12345678901234567,-98765432109876543]}');
    expect(result).not.toBeNull();
    expect(result?.ids).toEqual(['12345678901234567', '-98765432109876543']);
  });

  it('leaves normal small numbers untouched', () => {
    const result = bigIntAsStringJsonParse<{ a: number; b: number }>('{"a":123,"b":-456}');
    expect(result).toEqual({ a: 123, b: -456 });
  });

  it('leaves strings untouched', () => {
    const result = bigIntAsStringJsonParse<{ s: string }>('{"s":"12345678901234567"}');
    expect(result).toEqual({ s: '12345678901234567' });
  });
});
