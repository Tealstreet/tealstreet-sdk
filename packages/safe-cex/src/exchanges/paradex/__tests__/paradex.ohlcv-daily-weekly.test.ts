import { describe, it, expect, beforeAll } from 'vitest';
import { ParadexExchange } from '../paradex.exchange';
import { DefaultStore } from '../../../store/store.base';

describe('Paradex OHLCV - Daily and Weekly Candles', () => {
  let exchange: ParadexExchange;

  beforeAll(async () => {
    const store = new DefaultStore();
    exchange = new ParadexExchange(
      'paradex',
      {
        key: '',
        secret: '',
        testnet: true,
        publicDataOnly: true,
      },
      store,
    );

    await exchange.start();
  });

  it('should fetch 1d (daily) candles using TradingView endpoint', async () => {
    const symbol = 'BTC-USD-PERP';
    const now = Date.now();
    const tenDaysAgo = now - 10 * 24 * 60 * 60 * 1000;

    const candles = await exchange.fetchOHLCV({
      symbol,
      interval: '1d',
      from: tenDaysAgo,
      to: now,
    });

    console.log(`Fetched ${candles.length} daily candles`);
    if (candles.length > 0) {
      console.log('First candle:', candles[0]);
      console.log('Last candle:', candles[candles.length - 1]);
    }

    expect(candles).toBeDefined();
    expect(Array.isArray(candles)).toBe(true);
    expect(candles.length).toBeGreaterThan(0);

    // Verify candle structure
    const firstCandle = candles[0];
    expect(firstCandle).toHaveProperty('timestamp');
    expect(firstCandle).toHaveProperty('open');
    expect(firstCandle).toHaveProperty('high');
    expect(firstCandle).toHaveProperty('low');
    expect(firstCandle).toHaveProperty('close');
    expect(firstCandle).toHaveProperty('volume');

    // Daily candles should be ~24 hours apart (allowing for some variation)
    if (candles.length > 1) {
      const timeDiff = candles[1].timestamp - candles[0].timestamp;
      const hoursDiff = timeDiff / (1000 * 60 * 60);
      console.log(`Time between first two candles: ${hoursDiff} hours`);
      expect(hoursDiff).toBeGreaterThanOrEqual(20); // At least 20 hours
      expect(hoursDiff).toBeLessThanOrEqual(28); // At most 28 hours
    }
  });

  it(
    'should fetch 4h candles using TradingView endpoint',
    async () => {
      const symbol = 'BTC-USD-PERP';
      const now = Date.now();
      const twoDaysAgo = now - 2 * 24 * 60 * 60 * 1000;

      const candles = await exchange.fetchOHLCV({
        symbol,
        interval: '4h',
        from: twoDaysAgo,
        to: now,
      });

      console.log(`Fetched ${candles.length} 4-hour candles`);
      if (candles.length > 0) {
        console.log('First candle:', candles[0]);
        console.log('Last candle:', candles[candles.length - 1]);
      }

      expect(candles).toBeDefined();
      expect(Array.isArray(candles)).toBe(true);
      expect(candles.length).toBeGreaterThan(0);

      // 4-hour candles should be ~4 hours apart
      if (candles.length > 1) {
        const timeDiff = candles[1].timestamp - candles[0].timestamp;
        const hoursDiff = timeDiff / (1000 * 60 * 60);
        console.log(`Time between first two candles: ${hoursDiff} hours`);
        expect(hoursDiff).toBeGreaterThanOrEqual(3.5);
        expect(hoursDiff).toBeLessThanOrEqual(4.5);
      }
    },
    10000,
  ); // 10 second timeout

  it('should fetch 1h candles (maximum supported interval)', async () => {
    const symbol = 'BTC-USD-PERP';
    const now = Date.now();
    const twoDaysAgo = now - 2 * 24 * 60 * 60 * 1000;

    const candles = await exchange.fetchOHLCV({
      symbol,
      interval: '1h',
      from: twoDaysAgo,
      to: now,
    });

    console.log(`Fetched ${candles.length} 1-hour candles`);

    expect(candles).toBeDefined();
    expect(Array.isArray(candles)).toBe(true);
    expect(candles.length).toBeGreaterThan(0);

    // Verify candle structure
    const firstCandle = candles[0];
    expect(firstCandle).toHaveProperty('timestamp');
    expect(firstCandle).toHaveProperty('open');
    expect(firstCandle).toHaveProperty('high');
    expect(firstCandle).toHaveProperty('low');
    expect(firstCandle).toHaveProperty('close');
    expect(firstCandle).toHaveProperty('volume');

    console.log('First candle:', firstCandle);
    console.log('Last candle:', candles[candles.length - 1]);
  });

  it('should fetch 5m candles (commonly used interval)', async () => {
    const symbol = 'BTC-USD-PERP';
    const now = Date.now();
    const oneHourAgo = now - 60 * 60 * 1000;

    const candles = await exchange.fetchOHLCV({
      symbol,
      interval: '5m',
      from: oneHourAgo,
      to: now,
    });

    console.log(`Fetched ${candles.length} 5-minute candles`);

    expect(candles).toBeDefined();
    expect(Array.isArray(candles)).toBe(true);
    expect(candles.length).toBeGreaterThan(0);

    // 5-minute candles should be ~5 minutes apart
    if (candles.length > 1) {
      const timeDiff = candles[1].timestamp - candles[0].timestamp;
      const minutesDiff = timeDiff / (1000 * 60);
      console.log(`Time between first two candles: ${minutesDiff} minutes`);
      expect(minutesDiff).toBeGreaterThanOrEqual(4);
      expect(minutesDiff).toBeLessThanOrEqual(6);
    }
  });

  it('should fetch 2h candles (120 minutes)', async () => {
    const symbol = 'BTC-USD-PERP';
    const now = Date.now();
    const oneDayAgo = now - 24 * 60 * 60 * 1000;

    const candles = await exchange.fetchOHLCV({
      symbol,
      interval: '2h',
      from: oneDayAgo,
      to: now,
    });

    console.log(`Fetched ${candles.length} 2-hour candles`);

    expect(candles).toBeDefined();
    expect(Array.isArray(candles)).toBe(true);
    expect(candles.length).toBeGreaterThan(0);

    // 2-hour candles should be ~2 hours apart
    if (candles.length > 1) {
      const timeDiff = candles[1].timestamp - candles[0].timestamp;
      const hoursDiff = timeDiff / (1000 * 60 * 60);
      console.log(`Time between first two candles: ${hoursDiff} hours`);
      expect(hoursDiff).toBeGreaterThanOrEqual(1.5);
      expect(hoursDiff).toBeLessThanOrEqual(2.5);
    }
  });
});
