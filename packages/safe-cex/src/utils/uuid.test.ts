import { describe, it, expect } from 'vitest';
import { uuid } from './uuid';

describe('uuid', () => {
  it('should return a string', () => {
    const result = uuid();
    expect(typeof result).toBe('string');
  });

  it('should return non-empty string', () => {
    const result = uuid();
    expect(result.length).toBeGreaterThan(0);
  });

  it('should not contain hyphens', () => {
    const result = uuid();
    expect(result).not.toContain('-');
  });

  it('should not contain underscores', () => {
    const result = uuid();
    expect(result).not.toContain('_');
  });

  it('should generate unique values', () => {
    const ids = new Set<string>();
    for (let i = 0; i < 100; i++) {
      ids.add(uuid());
    }
    expect(ids.size).toBe(100);
  });

  it('should generate alphanumeric strings', () => {
    const result = uuid();
    expect(result).toMatch(/^[a-zA-Z0-9]+$/);
  });
});
