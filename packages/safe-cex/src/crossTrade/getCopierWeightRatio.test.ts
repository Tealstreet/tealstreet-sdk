import type { Exchange } from '../exchanges/base';
import type { Balance, Market, Ticker } from '../types';
import type { CrossTradeConfig, CrossTradeCopier } from './types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { getCopierWeightRatio } from './getCopierWeightRatio';

// Mock external dependencies
vi.mock('@tealstreet/utils/src/currency/currencyUtils', () => ({
  isBtcLikeSymbol: vi.fn((symbol: string) => symbol.includes('BTC')),
  isEquivalentCurrency: vi.fn((a: string, b: string) => a === b),
}));

vi.mock('../utils/getTotalBalanceFromBalance', () => ({
  getTotalBalanceFromBalance: vi.fn((balance: Balance, _market: Market) => {
    return balance?.total || 0;
  }),
}));

vi.mock('../utils/sizing', () => ({
  getMarginCurrency: vi.fn((market: Market) => market?.quote || 'USDT'),
  convertFromMarginToQuote: vi.fn((margin: number, _market: Market, lastPrice?: number, _btcPrice?: number) => {
    // Simple mock: if lastPrice provided, multiply
    return lastPrice ? margin * lastPrice : margin;
  }),
}));

// Helper to create mock balance
const createMockBalance = (total: number): Balance => ({
  total,
  free: total * 0.8,
  used: total * 0.2,
  upnl: 0,
});

// Helper to create mock market
const createMockMarket = (overrides: Partial<Market> = {}): Market =>
  ({
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { amount: 3, price: 2 },
    limits: { amount: { min: 0.001, max: 1000 } },
    ...overrides,
  }) as Market;

// Helper to create mock ticker
const createMockTicker = (symbol: string, last: number): Ticker =>
  ({
    symbol,
    last,
    bid: last - 10,
    ask: last + 10,
    percentage: 0,
    timestamp: Date.now(),
  }) as unknown as Ticker;

// Helper to create mock exchange
const createMockExchange = (
  balance: Balance,
  tickers: Ticker[] = [],
  tickersBySymbol: Record<string, Ticker> = {},
): Exchange =>
  ({
    store: {
      balance,
      tickers,
      tickersBySymbol,
    },
  }) as unknown as Exchange;

// Helper to create config
const createMockConfig = (overrides: Partial<CrossTradeConfig> = {}): CrossTradeConfig => ({
  enabled: true,
  masterAccount: 'master',
  masterSymbol: undefined,
  masterWeight: 1,
  copiers: [],
  ...overrides,
});

// Helper to create copier
const createMockCopier = (overrides: Partial<CrossTradeCopier> = {}): CrossTradeCopier => ({
  enabled: true,
  account: 'copier',
  weight: 1,
  execution: 'LimitSlippage',
  weightMode: 'Absolute',
  symbolMappings: {},
  ...overrides,
});

describe('getCopierWeightRatio', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Absolute weight mode', () => {
    it('should return simple ratio when weightMode is Absolute', () => {
      const config = createMockConfig({ masterWeight: 1 });
      const copier = createMockCopier({ weight: 2, weightMode: 'Absolute' });
      const masterExchange = createMockExchange(createMockBalance(10000));
      const copierExchange = createMockExchange(createMockBalance(5000));
      const masterMarket = createMockMarket();
      const copierMarket = createMockMarket();

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      expect(result).toBe(2); // copier.weight / masterWeight = 2 / 1
    });

    it('should handle master weight greater than 1', () => {
      const config = createMockConfig({ masterWeight: 2 });
      const copier = createMockCopier({ weight: 4, weightMode: 'Absolute' });
      const masterExchange = createMockExchange(createMockBalance(10000));
      const copierExchange = createMockExchange(createMockBalance(5000));
      const masterMarket = createMockMarket();
      const copierMarket = createMockMarket();

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      expect(result).toBe(2); // 4 / 2 = 2
    });

    it('should handle fractional weights', () => {
      const config = createMockConfig({ masterWeight: 1 });
      const copier = createMockCopier({ weight: 0.5, weightMode: 'Absolute' });
      const masterExchange = createMockExchange(createMockBalance(10000));
      const copierExchange = createMockExchange(createMockBalance(5000));
      const masterMarket = createMockMarket();
      const copierMarket = createMockMarket();

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      expect(result).toBe(0.5);
    });

    it('should default masterWeight to 1 when undefined', () => {
      const config = createMockConfig({ masterWeight: undefined });
      const copier = createMockCopier({ weight: 3, weightMode: 'Absolute' });
      const masterExchange = createMockExchange(createMockBalance(10000));
      const copierExchange = createMockExchange(createMockBalance(5000));
      const masterMarket = createMockMarket();
      const copierMarket = createMockMarket();

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      expect(result).toBe(3);
    });
  });

  describe('Balance weight mode with same currency', () => {
    it('should calculate ratio based on balance when currencies match', () => {
      const config = createMockConfig({ masterWeight: 1 });
      const copier = createMockCopier({ weight: 1, weightMode: 'Balance' });
      const masterExchange = createMockExchange(createMockBalance(10000));
      const copierExchange = createMockExchange(createMockBalance(5000));
      const masterMarket = createMockMarket({ quote: 'USDT' });
      const copierMarket = createMockMarket({ quote: 'USDT' });

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      // weight * (copierBalance / masterBalance) = 1 * (5000 / 10000) = 0.5
      expect(result).toBe(0.5);
    });

    it('should multiply by weight when balance mode', () => {
      const config = createMockConfig({ masterWeight: 1 });
      const copier = createMockCopier({ weight: 2, weightMode: 'Balance' });
      const masterExchange = createMockExchange(createMockBalance(10000));
      const copierExchange = createMockExchange(createMockBalance(5000));
      const masterMarket = createMockMarket({ quote: 'USDT' });
      const copierMarket = createMockMarket({ quote: 'USDT' });

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      // weight * (copierBalance / masterBalance) = 2 * (5000 / 10000) = 1.0
      expect(result).toBe(1);
    });

    it('should return 0 when copier balance is 0', () => {
      const config = createMockConfig({ masterWeight: 1 });
      const copier = createMockCopier({ weight: 1, weightMode: 'Balance' });
      const masterExchange = createMockExchange(createMockBalance(10000));
      const copierExchange = createMockExchange(createMockBalance(0));
      const masterMarket = createMockMarket({ quote: 'USDT' });
      const copierMarket = createMockMarket({ quote: 'USDT' });

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      expect(result).toBe(0);
    });

    it('should return 0 when master balance is 0', () => {
      const config = createMockConfig({ masterWeight: 1 });
      const copier = createMockCopier({ weight: 1, weightMode: 'Balance' });
      const masterExchange = createMockExchange(createMockBalance(0));
      const copierExchange = createMockExchange(createMockBalance(5000));
      const masterMarket = createMockMarket({ quote: 'USDT' });
      const copierMarket = createMockMarket({ quote: 'USDT' });

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      expect(result).toBe(0);
    });
  });

  describe('Balance weight mode with different currencies', () => {
    it('should convert to quote currency when currencies differ', () => {
      const config = createMockConfig({ masterWeight: 1 });
      const copier = createMockCopier({ weight: 1, weightMode: 'Balance' });

      const masterTickers = [createMockTicker('BTCUSDT', 50000)];
      const copierTickers = [createMockTicker('ETHUSDC', 3000)];

      const masterExchange = createMockExchange(createMockBalance(10000), masterTickers, { BTCUSDT: masterTickers[0] });
      const copierExchange = createMockExchange(createMockBalance(5000), copierTickers, { ETHUSDC: copierTickers[0] });

      const masterMarket = createMockMarket({ symbol: 'BTCUSDT', quote: 'USDT' });
      const copierMarket = createMockMarket({ symbol: 'ETHUSDC', quote: 'USDC' });

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      // Mock converts using lastPrice, so values get multiplied
      // copierBalanceInQuote = 5000 * 3000 = 15000000
      // masterBalanceInQuote = 10000 * 50000 = 500000000
      // ratio = 15000000 / 500000000 = 0.03
      expect(result).toBeCloseTo(0.03);
    });
  });

  describe('edge cases', () => {
    it('should return absolute value (never negative)', () => {
      const config = createMockConfig({ masterWeight: 1 });
      const copier = createMockCopier({ weight: -2, weightMode: 'Absolute' });
      const masterExchange = createMockExchange(createMockBalance(10000));
      const copierExchange = createMockExchange(createMockBalance(5000));
      const masterMarket = createMockMarket();
      const copierMarket = createMockMarket();

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      expect(result).toBe(2); // Math.abs(-2)
    });

    it('should handle zero weight', () => {
      const config = createMockConfig({ masterWeight: 1 });
      const copier = createMockCopier({ weight: 0, weightMode: 'Absolute' });
      const masterExchange = createMockExchange(createMockBalance(10000));
      const copierExchange = createMockExchange(createMockBalance(5000));
      const masterMarket = createMockMarket();
      const copierMarket = createMockMarket();

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      expect(result).toBe(0);
    });

    it('should handle very large weight ratios', () => {
      const config = createMockConfig({ masterWeight: 0.001 });
      const copier = createMockCopier({ weight: 1000, weightMode: 'Absolute' });
      const masterExchange = createMockExchange(createMockBalance(10000));
      const copierExchange = createMockExchange(createMockBalance(5000));
      const masterMarket = createMockMarket();
      const copierMarket = createMockMarket();

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      expect(result).toBe(1000000); // 1000 / 0.001
    });

    it('should handle equal balances in Balance mode', () => {
      const config = createMockConfig({ masterWeight: 1 });
      const copier = createMockCopier({ weight: 1, weightMode: 'Balance' });
      const masterExchange = createMockExchange(createMockBalance(10000));
      const copierExchange = createMockExchange(createMockBalance(10000));
      const masterMarket = createMockMarket({ quote: 'USDT' });
      const copierMarket = createMockMarket({ quote: 'USDT' });

      const result = getCopierWeightRatio(config, copier, masterExchange, copierExchange, masterMarket, copierMarket);

      expect(result).toBe(1); // Equal balances with weight 1
    });
  });
});
