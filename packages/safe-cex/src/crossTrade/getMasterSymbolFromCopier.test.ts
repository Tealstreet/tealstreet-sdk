import type { CrossTradeConfig, CrossTradeCopier } from './types';

import { describe, expect, it } from 'vitest';

import { getMasterSymbolFromCopier } from './getMasterSymbolFromCopier';

const createMockConfig = (overrides: Partial<CrossTradeConfig> = {}): CrossTradeConfig => ({
  enabled: true,
  masterAccount: 'master',
  masterSymbol: undefined,
  masterWeight: 1,
  copiers: [],
  ...overrides,
});

const createMockCopier = (overrides: Partial<CrossTradeCopier> = {}): CrossTradeCopier => ({
  enabled: true,
  account: 'copier',
  weight: 1,
  execution: 'LimitSlippage',
  weightMode: 'Absolute',
  symbolMappings: {},
  ...overrides,
});

describe('getMasterSymbolFromCopier', () => {
  describe('fixed master symbol from config', () => {
    it('should return masterSymbol from config when set', () => {
      const config = createMockConfig({ masterSymbol: 'BTCUSDT' });
      const copier = createMockCopier({
        symbolMappings: { BTCUSDT: 'BTC-PERP' },
      });

      const result = getMasterSymbolFromCopier({
        config,
        copier,
        masterExchangeName: 'binance',
        copierExchangeName: 'bybit',
        copierSymbol: 'BTC-PERP',
      });

      expect(result).toBe('BTCUSDT');
    });
  });

  describe('reversed symbol mappings', () => {
    it('should use reversed symbol mapping when copier symbol is found', () => {
      const config = createMockConfig({ masterSymbol: undefined });
      const copier = createMockCopier({
        symbolMappings: { BTCUSDT: 'BTC-PERP' },
      });

      const result = getMasterSymbolFromCopier({
        config,
        copier,
        masterExchangeName: 'binance',
        copierExchangeName: 'bybit',
        copierSymbol: 'BTC-PERP',
      });

      expect(result).toBe('BTCUSDT');
    });
  });

  describe('symbol conversion fallback', () => {
    it('should convert bybit → binance when no mapping exists', () => {
      const config = createMockConfig({ masterSymbol: undefined });
      const copier = createMockCopier({ symbolMappings: {} });

      const result = getMasterSymbolFromCopier({
        config,
        copier,
        masterExchangeName: 'binance',
        copierExchangeName: 'bybit',
        copierSymbol: 'BTCUSDT',
      });

      expect(result).toBe('BTCUSDT');
    });

    it('should convert hyperliquid → binance', () => {
      const config = createMockConfig({ masterSymbol: undefined });
      const copier = createMockCopier({ symbolMappings: {} });

      const result = getMasterSymbolFromCopier({
        config,
        copier,
        masterExchangeName: 'binance',
        copierExchangeName: 'hyperliquid',
        copierSymbol: 'BTC-PERP',
      });

      expect(result).toBe('BTCUSDC');
    });
  });

  describe('edge cases', () => {
    it('should handle undefined masterExchangeName', () => {
      const config = createMockConfig({ masterSymbol: undefined });
      const copier = createMockCopier({ symbolMappings: {} });

      const result = getMasterSymbolFromCopier({
        config,
        copier,
        masterExchangeName: undefined,
        copierExchangeName: 'bybit',
        copierSymbol: 'BTCUSDT',
      });

      expect(result).toBe('BTCUSDT');
    });

    it('should handle same exchange', () => {
      const config = createMockConfig({ masterSymbol: undefined });
      const copier = createMockCopier({ symbolMappings: {} });

      const result = getMasterSymbolFromCopier({
        config,
        copier,
        masterExchangeName: 'binance',
        copierExchangeName: 'binance',
        copierSymbol: 'BTCUSDT',
      });

      expect(result).toBe('BTCUSDT');
    });

    it('should follow priority: masterSymbol > reversedMapping > conversion', () => {
      const copier = createMockCopier({
        symbolMappings: { BTCUSDT: 'BTC-PERP' },
      });

      // masterSymbol wins
      expect(
        getMasterSymbolFromCopier({
          config: createMockConfig({ masterSymbol: 'FIXED' }),
          copier,
          masterExchangeName: 'binance',
          copierExchangeName: 'bybit',
          copierSymbol: 'BTC-PERP',
        }),
      ).toBe('FIXED');

      // reversed mapping wins over conversion
      expect(
        getMasterSymbolFromCopier({
          config: createMockConfig({ masterSymbol: undefined }),
          copier,
          masterExchangeName: 'binance',
          copierExchangeName: 'bybit',
          copierSymbol: 'BTC-PERP',
        }),
      ).toBe('BTCUSDT');
    });
  });
});
