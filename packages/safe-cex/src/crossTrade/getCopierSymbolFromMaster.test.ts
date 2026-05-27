import type { CrossTradeCopier } from './types';

import { describe, expect, it } from 'vitest';

import { getCopierSymbolFromMaster } from './getCopierSymbolFromMaster';

const createMockCopier = (overrides: Partial<CrossTradeCopier> = {}): CrossTradeCopier => ({
  enabled: true,
  account: 'test-account',
  weight: 1,
  execution: 'LimitSlippage',
  weightMode: 'Absolute',
  symbolMappings: {},
  ...overrides,
});

describe('getCopierSymbolFromMaster', () => {
  describe('user-configured symbol mappings', () => {
    it('should return user-configured symbol mapping when present', () => {
      const copier = createMockCopier({
        symbolMappings: { BTCUSDT: 'BTC-PERP' },
      });

      const result = getCopierSymbolFromMaster({
        copier,
        copierExchangeName: 'bybit',
        masterExchangeName: 'binance',
        masterSymbol: 'BTCUSDT',
      });

      expect(result).toBe('BTC-PERP');
    });

    it('should prioritize symbol mapping over conversion', () => {
      const copier = createMockCopier({
        symbolMappings: { ETHUSDT: 'ETH-CUSTOM' },
      });

      const result = getCopierSymbolFromMaster({
        copier,
        copierExchangeName: 'bybit',
        masterExchangeName: 'binance',
        masterSymbol: 'ETHUSDT',
      });

      expect(result).toBe('ETH-CUSTOM');
    });
  });

  describe('symbol conversion fallback', () => {
    it('should convert binance → bybit (passthrough)', () => {
      const copier = createMockCopier({ symbolMappings: {} });

      const result = getCopierSymbolFromMaster({
        copier,
        copierExchangeName: 'bybit',
        masterExchangeName: 'binance',
        masterSymbol: 'BTCUSDT',
      });

      expect(result).toBe('BTCUSDT');
    });

    it('should convert binance → hyperliquid', () => {
      const copier = createMockCopier({ symbolMappings: {} });

      const result = getCopierSymbolFromMaster({
        copier,
        copierExchangeName: 'hyperliquid',
        masterExchangeName: 'binance',
        masterSymbol: 'BTCUSDC',
      });

      expect(result).toBe('BTC-PERP');
    });

    it('should convert hyperliquid → binance', () => {
      const copier = createMockCopier({ symbolMappings: {} });

      const result = getCopierSymbolFromMaster({
        copier,
        copierExchangeName: 'binance',
        masterExchangeName: 'hyperliquid',
        masterSymbol: 'BTC-PERP',
      });

      expect(result).toBe('BTCUSDC');
    });
  });

  describe('edge cases', () => {
    it('should handle undefined copierExchangeName', () => {
      const copier = createMockCopier({ symbolMappings: {} });

      const result = getCopierSymbolFromMaster({
        copier,
        copierExchangeName: undefined,
        masterExchangeName: 'binance',
        masterSymbol: 'BTCUSDT',
      });

      expect(result).toBe('BTCUSDT');
    });

    it('should handle same exchange for master and copier', () => {
      const copier = createMockCopier({ symbolMappings: {} });

      const result = getCopierSymbolFromMaster({
        copier,
        copierExchangeName: 'binance',
        masterExchangeName: 'binance',
        masterSymbol: 'BTCUSDT',
      });

      expect(result).toBe('BTCUSDT');
    });
  });
});
