import { describe, it, expect, beforeAll } from 'vitest';
import { ParadexExchange } from '../paradex.exchange';
import { DefaultStore } from '../../../store/store.base';
import axios from 'axios';

/**
 * Test suite for Paradex TradingView-specific endpoint
 *
 * This endpoint appears to support more intervals than the standard /markets/klines endpoint
 * Endpoint: https://api.prod.paradex.trade/v1/tradingview/history
 *
 * Parameters:
 * - symbol: Market symbol (e.g., BTC-USD-PERP)
 * - resolution: TradingView format (1, 5, 15, 30, 60, 1D, 1W, 1M)
 * - from: Unix timestamp in seconds
 * - to: Unix timestamp in seconds
 * - countback: Number of candles to return
 * - price_kind: Type of price (last, mark, index)
 */
describe('Paradex TradingView Endpoint Investigation', () => {
  const BASE_URL_TESTNET = 'https://api.testnet.paradex.trade/v1/tradingview/history';
  const BASE_URL_PROD = 'https://api.prod.paradex.trade/v1/tradingview/history';

  it('should fetch 1D (daily) candles using TradingView endpoint - testnet', async () => {
    const now = Math.floor(Date.now() / 1000); // Unix seconds
    const tenDaysAgo = now - 10 * 24 * 60 * 60;

    const params = {
      symbol: 'BTC-USD-PERP',
      resolution: '1D',
      from: tenDaysAgo,
      to: now,
      countback: 100,
      price_kind: 'last',
    };

    console.log('Testing testnet endpoint with params:', params);

    try {
      const response = await axios.get(BASE_URL_TESTNET, { params });
      console.log('Response status:', response.status);
      console.log('Response data keys:', Object.keys(response.data));
      console.log('Response data:', JSON.stringify(response.data, null, 2));

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
    } catch (error: any) {
      console.error('Error:', error.message);
      if (error.response) {
        console.error('Response status:', error.response.status);
        console.error('Response data:', error.response.data);
      }
      throw error;
    }
  }, 10000);

  it('should fetch 1W (weekly) candles using TradingView endpoint - testnet', async () => {
    const now = Math.floor(Date.now() / 1000);
    const tenWeeksAgo = now - 10 * 7 * 24 * 60 * 60;

    const params = {
      symbol: 'BTC-USD-PERP',
      resolution: '1W',
      from: tenWeeksAgo,
      to: now,
      countback: 100,
      price_kind: 'last',
    };

    console.log('Testing weekly candles with params:', params);

    try {
      const response = await axios.get(BASE_URL_TESTNET, { params });
      console.log('Response status:', response.status);
      console.log('Response data:', JSON.stringify(response.data, null, 2));

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
    } catch (error: any) {
      console.error('Error:', error.message);
      if (error.response) {
        console.error('Response status:', error.response.status);
        console.error('Response data:', error.response.data);
      }
      throw error;
    }
  }, 10000);

  it('should fetch 4H candles using TradingView endpoint - testnet', async () => {
    const now = Math.floor(Date.now() / 1000);
    const twoDaysAgo = now - 2 * 24 * 60 * 60;

    const params = {
      symbol: 'BTC-USD-PERP',
      resolution: '240', // TradingView uses minutes for intraday (4h = 240m)
      from: twoDaysAgo,
      to: now,
      countback: 100,
      price_kind: 'last',
    };

    console.log('Testing 4H candles with params:', params);

    try {
      const response = await axios.get(BASE_URL_TESTNET, { params });
      console.log('Response status:', response.status);
      console.log('Response data:', JSON.stringify(response.data, null, 2));

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
    } catch (error: any) {
      console.error('Error:', error.message);
      if (error.response) {
        console.error('Response status:', error.response.status);
        console.error('Response data:', error.response.data);
      }
      throw error;
    }
  }, 10000);

  it('should test various resolution formats', async () => {
    const now = Math.floor(Date.now() / 1000);
    const oneDayAgo = now - 24 * 60 * 60;

    const resolutions = [
      '1',    // 1 minute
      '5',    // 5 minutes
      '15',   // 15 minutes
      '60',   // 1 hour
      '240',  // 4 hours
      '1D',   // 1 day
      '1W',   // 1 week
    ];

    for (const resolution of resolutions) {
      console.log(`\n=== Testing resolution: ${resolution} ===`);

      const params = {
        symbol: 'BTC-USD-PERP',
        resolution,
        from: oneDayAgo,
        to: now,
        countback: 10,
        price_kind: 'last',
      };

      try {
        const response = await axios.get(BASE_URL_TESTNET, { params });
        console.log(`✓ Resolution ${resolution}: Success`);
        console.log(`  Status: ${response.status}`);
        console.log(`  Data structure:`, Object.keys(response.data));

        if (response.data.t) {
          console.log(`  Candles returned: ${response.data.t.length}`);
          if (response.data.t.length > 0) {
            console.log(`  First timestamp: ${response.data.t[0]} (${new Date(response.data.t[0] * 1000).toISOString()})`);
            console.log(`  Last timestamp: ${response.data.t[response.data.t.length - 1]} (${new Date(response.data.t[response.data.t.length - 1] * 1000).toISOString()})`);
          }
        }
      } catch (error: any) {
        console.log(`✗ Resolution ${resolution}: Failed`);
        if (error.response) {
          console.log(`  Status: ${error.response.status}`);
          console.log(`  Error:`, error.response.data);
        } else {
          console.log(`  Error:`, error.message);
        }
      }
    }
  }, 30000);

  it('should understand the response format', async () => {
    const now = Math.floor(Date.now() / 1000);
    const oneHourAgo = now - 60 * 60;

    const params = {
      symbol: 'BTC-USD-PERP',
      resolution: '5', // 5 minutes
      from: oneHourAgo,
      to: now,
      countback: 20,
      price_kind: 'last',
    };

    const response = await axios.get(BASE_URL_TESTNET, { params });
    console.log('Full response for 5m candles:', JSON.stringify(response.data, null, 2));

    // TradingView format is typically:
    // { s: 'ok', t: [timestamps], o: [opens], h: [highs], l: [lows], c: [closes], v: [volumes] }
    expect(response.data).toHaveProperty('s'); // status

    if (response.data.s === 'ok') {
      expect(response.data).toHaveProperty('t'); // timestamps
      expect(response.data).toHaveProperty('o'); // opens
      expect(response.data).toHaveProperty('h'); // highs
      expect(response.data).toHaveProperty('l'); // lows
      expect(response.data).toHaveProperty('c'); // closes
      expect(response.data).toHaveProperty('v'); // volumes

      console.log('Response structure confirmed:');
      console.log('- Status:', response.data.s);
      console.log('- Candles count:', response.data.t?.length);
      console.log('- Has OHLCV arrays:', {
        timestamps: Array.isArray(response.data.t),
        opens: Array.isArray(response.data.o),
        highs: Array.isArray(response.data.h),
        lows: Array.isArray(response.data.l),
        closes: Array.isArray(response.data.c),
        volumes: Array.isArray(response.data.v),
      });
    } else {
      console.log('Status not ok:', response.data);
    }
  }, 10000);
});
