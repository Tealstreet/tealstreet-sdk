import { describe, it, expect } from 'vitest';
import { v, vn } from './get-key';

describe('get-key', () => {
  describe('v (single key lookup)', () => {
    it('should return value for exact key match', () => {
      const obj = { symbol: 'BTCUSDT', price: 100 };
      expect(v(obj, 'symbol')).toBe('BTCUSDT');
      expect(v(obj, 'price')).toBe(100);
    });

    it('should return value for snake_case key when given camelCase', () => {
      const obj = { order_type: 'limit', side_type: 'buy' };
      expect(v(obj, 'orderType')).toBe('limit');
      expect(v(obj, 'sideType')).toBe('buy');
    });

    it('should return value for camelCase key when given snake_case', () => {
      const obj = { orderType: 'limit', sideType: 'buy' };
      expect(v(obj, 'order_type')).toBe('limit');
      expect(v(obj, 'side_type')).toBe('buy');
    });

    it('should return undefined for non-existent key', () => {
      const obj = { symbol: 'BTCUSDT' };
      expect(v(obj, 'price')).toBeUndefined();
    });

    it('should return undefined for null object', () => {
      expect(v(null as any, 'key')).toBeUndefined();
    });

    it('should return undefined for undefined object', () => {
      expect(v(undefined as any, 'key')).toBeUndefined();
    });

    it('should handle falsy values in object', () => {
      const obj = { value: 0, empty: '', flag: false, nil: null };
      expect(v(obj, 'value')).toBe(0);
      expect(v(obj, 'empty')).toBe('');
      expect(v(obj, 'flag')).toBe(false);
      expect(v(obj, 'nil')).toBeNull();
    });

    it('should prioritize exact match over case variants', () => {
      const obj = { orderType: 'camel', order_type: 'snake' };
      expect(v(obj, 'orderType')).toBe('camel');
      expect(v(obj, 'order_type')).toBe('snake');
    });
  });

  describe('vn (multiple key lookup)', () => {
    it('should return first found value', () => {
      const obj = { symbol: 'BTCUSDT', price: 100 };
      expect(vn(obj, 'symbol', 'price')).toBe('BTCUSDT');
    });

    it('should try keys in order', () => {
      const obj = { price: 100 };
      expect(vn(obj, 'missing', 'price')).toBe(100);
    });

    it('should handle case variations in key search', () => {
      const obj = { order_type: 'limit' };
      expect(vn(obj, 'orderType')).toBe('limit');
    });

    it('should return undefined if no keys match', () => {
      const obj = { symbol: 'BTCUSDT' };
      expect(vn(obj, 'missing1', 'missing2', 'missing3')).toBeUndefined();
    });

    it('should stop at first defined value', () => {
      const obj = { first: undefined, second: 'found' };
      // Note: 'first' key exists but has undefined value
      expect(vn(obj, 'first', 'second')).toBe('found');
    });

    it('should handle single key', () => {
      const obj = { symbol: 'BTCUSDT' };
      expect(vn(obj, 'symbol')).toBe('BTCUSDT');
    });

    it('should handle no keys', () => {
      const obj = { symbol: 'BTCUSDT' };
      expect(vn(obj)).toBeUndefined();
    });

    it('should work with case variants across multiple keys', () => {
      const obj = { base_currency: 'BTC' };
      expect(vn(obj, 'quoteCurrency', 'baseCurrency')).toBe('BTC');
    });
  });
});
