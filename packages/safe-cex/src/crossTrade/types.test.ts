import { describe, it, expect } from 'vitest';
import {
  defaultCrossTradeCopier,
  defaultCrossTradeConfig,
  type CrossTradeCopier,
  type CrossTradeConfig,
  type CrossTradWeightMode,
  type CrossTradeExecution,
} from './types';

describe('crossTrade types', () => {
  describe('defaultCrossTradeCopier', () => {
    it('should have correct default values', () => {
      expect(defaultCrossTradeCopier.enabled).toBe(false);
      expect(defaultCrossTradeCopier.account).toBeUndefined();
      expect(defaultCrossTradeCopier.weight).toBe(1);
      expect(defaultCrossTradeCopier.execution).toBe('LimitSlippage');
      expect(defaultCrossTradeCopier.weightMode).toBe('Absolute');
      expect(defaultCrossTradeCopier.symbolMappings).toEqual({});
    });

    it('should be frozen (immutable)', () => {
      expect(Object.isFrozen(defaultCrossTradeCopier)).toBe(true);
    });

    it('should throw when trying to modify frozen object', () => {
      expect(() => {
        (defaultCrossTradeCopier as any).enabled = true;
      }).toThrow();
    });
  });

  describe('defaultCrossTradeConfig', () => {
    it('should have correct default values', () => {
      expect(defaultCrossTradeConfig.enabled).toBe(false);
      expect(defaultCrossTradeConfig.masterAccount).toBeUndefined();
      expect(defaultCrossTradeConfig.masterSymbol).toBeUndefined();
      expect(defaultCrossTradeConfig.masterWeight).toBe(1);
      expect(defaultCrossTradeConfig.copiers).toEqual([]);
    });
  });

  describe('type compatibility', () => {
    it('should allow creating CrossTradeCopier from defaults', () => {
      const copier: CrossTradeCopier = {
        ...defaultCrossTradeCopier,
        enabled: true,
        account: 'test-account',
      };
      expect(copier.enabled).toBe(true);
      expect(copier.account).toBe('test-account');
      expect(copier.weight).toBe(1);
    });

    it('should allow creating CrossTradeConfig from defaults', () => {
      const config: CrossTradeConfig = {
        ...defaultCrossTradeConfig,
        enabled: true,
        masterAccount: 'master-account',
      };
      expect(config.enabled).toBe(true);
      expect(config.masterAccount).toBe('master-account');
    });

    it('should accept valid weight modes', () => {
      const modes: CrossTradWeightMode[] = ['Balance', 'Absolute'];
      modes.forEach(mode => {
        const copier: CrossTradeCopier = {
          ...defaultCrossTradeCopier,
          weightMode: mode,
        };
        expect(copier.weightMode).toBe(mode);
      });
    });

    it('should accept valid execution types', () => {
      const executions: CrossTradeExecution[] = ['LimitSlippage', 'Market', 'Chase'];
      executions.forEach(exec => {
        const copier: CrossTradeCopier = {
          ...defaultCrossTradeCopier,
          execution: exec,
        };
        expect(copier.execution).toBe(exec);
      });
    });
  });

  describe('symbol mappings', () => {
    it('should allow setting symbol mappings', () => {
      const copier: CrossTradeCopier = {
        ...defaultCrossTradeCopier,
        symbolMappings: {
          'BTCUSDT': 'XBTUSD',
          'ETHUSDT': 'ETHUSD',
        },
      };
      expect(copier.symbolMappings['BTCUSDT']).toBe('XBTUSD');
      expect(copier.symbolMappings['ETHUSDT']).toBe('ETHUSD');
    });
  });
});
