import { describe, it, expect } from 'vitest';
import { clone } from './clone';

describe('clone', () => {
  describe('primitive values', () => {
    it('should handle objects with primitive values', () => {
      const obj = { a: 1, b: 'string', c: true, d: null };
      const cloned = clone(obj);

      expect(cloned).toEqual(obj);
      expect(cloned).not.toBe(obj);
    });

    it('should handle nested primitive values', () => {
      const obj = {
        level1: {
          level2: {
            value: 42,
          },
        },
      };
      const cloned = clone(obj);

      expect(cloned).toEqual(obj);
      expect(cloned.level1).not.toBe(obj.level1);
      expect(cloned.level1.level2).not.toBe(obj.level1.level2);
    });
  });

  describe('arrays', () => {
    it('should clone arrays', () => {
      const obj = { arr: [1, 2, 3] };
      const cloned = clone(obj);

      expect(cloned.arr).toEqual([1, 2, 3]);
      expect(cloned.arr).not.toBe(obj.arr);
    });

    it('should clone nested arrays', () => {
      const obj = { arr: [[1, 2], [3, 4]] };
      const cloned = clone(obj);

      expect(cloned.arr).toEqual([[1, 2], [3, 4]]);
      expect(cloned.arr[0]).not.toBe(obj.arr[0]);
    });

    it('should clone arrays of objects', () => {
      const obj = { items: [{ a: 1 }, { b: 2 }] };
      const cloned = clone(obj);

      expect(cloned.items).toEqual([{ a: 1 }, { b: 2 }]);
      expect(cloned.items[0]).not.toBe(obj.items[0]);
    });
  });

  describe('deep objects', () => {
    it('should deeply clone nested objects', () => {
      const obj = {
        user: {
          name: 'John',
          settings: {
            theme: 'dark',
            notifications: {
              email: true,
              push: false,
            },
          },
        },
      };

      const cloned = clone(obj);

      expect(cloned).toEqual(obj);
      expect(cloned.user.settings.notifications).not.toBe(obj.user.settings.notifications);
    });

    it('should not mutate original when modifying clone', () => {
      const obj = { nested: { value: 1 } };
      const cloned = clone(obj);

      cloned.nested.value = 999;

      expect(obj.nested.value).toBe(1);
    });
  });

  describe('special types (structuredClone fallback)', () => {
    it('should clone Date objects', () => {
      const date = new Date('2024-01-15');
      const obj = { date };
      const cloned = clone(obj);

      expect(cloned.date).toEqual(date);
      expect(cloned.date).not.toBe(date);
      expect(cloned.date instanceof Date).toBe(true);
    });

    it('should clone Map objects', () => {
      const map = new Map([['key', 'value']]);
      const obj = { map } as { map: Map<string, string> };
      const cloned = clone(obj);

      expect(cloned.map.get('key')).toBe('value');
      expect(cloned.map).not.toBe(map);
    });

    it('should clone Set objects', () => {
      const set = new Set([1, 2, 3]);
      const obj = { set } as { set: Set<number> };
      const cloned = clone(obj);

      expect(cloned.set.has(1)).toBe(true);
      expect(cloned.set.has(2)).toBe(true);
      expect(cloned.set.has(3)).toBe(true);
      expect(cloned.set).not.toBe(set);
    });
  });

  describe('edge cases', () => {
    it('should handle empty objects', () => {
      const obj = {};
      const cloned = clone(obj);

      expect(cloned).toEqual({});
      expect(cloned).not.toBe(obj);
    });

    it('should handle empty arrays', () => {
      const obj = { arr: [] as number[] };
      const cloned = clone(obj);

      expect(cloned.arr).toEqual([]);
      expect(cloned.arr).not.toBe(obj.arr);
    });

    it('should handle null values in objects', () => {
      const obj = { a: null, b: { c: null } };
      const cloned = clone(obj);

      expect(cloned).toEqual(obj);
    });

    it('should handle undefined values in objects', () => {
      const obj = { a: undefined, b: 1 };
      const cloned = clone(obj);

      expect(cloned.a).toBeUndefined();
      expect(cloned.b).toBe(1);
    });

    it('should handle objects with many keys', () => {
      const obj: Record<string, number> = {};
      for (let i = 0; i < 100; i++) {
        obj[`key${i}`] = i;
      }

      const cloned = clone(obj);

      expect(Object.keys(cloned).length).toBe(100);
      expect(cloned.key50).toBe(50);
    });

    it('should handle deeply nested structures', () => {
      let obj: Record<string, any> = { value: 'deep' };
      for (let i = 0; i < 10; i++) {
        obj = { nested: obj };
      }

      const cloned = clone(obj);

      expect(cloned).toEqual(obj);
    });
  });
});
