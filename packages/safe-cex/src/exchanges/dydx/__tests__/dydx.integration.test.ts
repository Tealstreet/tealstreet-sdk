import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import { DydxExchange } from '../dydx.exchange';
import { ExchangeOptions } from '../../../types';
import { DefaultStore } from '../../../store/store.base';

/**
 * Integration tests for dYdX v4 Exchange
 *
 * IMPORTANT: dYdX v4 requires a MNEMONIC phrase for wallet initialization.
 * The SDK cannot work with just a private key - it needs the full 12-word mnemonic.
 *
 * To run these tests, update .apikey.json with:
 * {
 *   "apiKey": "dydx1...",  // Your dydx wallet address
 *   "secret": "word1 word2 word3 ...",  // Your 12-word mnemonic phrase
 *   "testnet": true  // or false for mainnet
 * }
 */

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet?: boolean;
}

describe.skip('DydxExchange - Integration Tests', () => {
  let apiConfig: ApiKeyConfig;
  let exchange: DydxExchange;
  let store: DefaultStore;

  beforeAll(() => {
    // Load API credentials from .apikey.json
    const configPath = join(__dirname, '.apikey.json');
    try {
      const configData = readFileSync(configPath, 'utf-8');
      apiConfig = JSON.parse(configData);
    } catch (error) {
      console.warn('No .apikey.json found or invalid format');
      return;
    }
  });

  it('should load API credentials correctly', () => {
    expect(apiConfig).toBeDefined();
    expect(apiConfig.apiKey).toBeTruthy();
    expect(apiConfig.secret).toBeTruthy();
    expect(apiConfig.apiKey).toMatch(/^dydx1[a-z0-9]+$/); // dydx bech32 address
  });

  it('should validate mnemonic format', () => {
    // dYdX v4 requires a mnemonic phrase (12 or 24 words)
    const words = apiConfig.secret.trim().split(/\s+/);
    expect([12, 24]).toContain(words.length);
  });

  it('should create dydx exchange instance', () => {
    const options: ExchangeOptions = {
      key: apiConfig.apiKey,
      secret: apiConfig.secret,
      dydxMnemonic: apiConfig.secret, // Same as secret for now
      testnet: apiConfig.testnet || false,
    };

    store = new DefaultStore();
    exchange = new DydxExchange('dydx', options, store);

    expect(exchange).toBeDefined();
    expect(exchange.options.key).toBe(apiConfig.apiKey);
  });

  it(
    'should validate account and initialize SDK',
    async () => {
      const options: ExchangeOptions = {
        key: apiConfig.apiKey,
        secret: apiConfig.secret,
        dydxMnemonic: apiConfig.secret,
        testnet: apiConfig.testnet || false,
      };

      store = new DefaultStore();
      exchange = new DydxExchange('dydx', options, store);

      // This will initialize the SDK and verify the wallet
      await exchange.validateAccount();

      expect((exchange as any).sdkClient).toBeDefined();
      expect((exchange as any).sdkClient.wallet).toBeDefined();
      expect((exchange as any).sdkClient.compositeClient).toBeDefined();
    },
    60000, // 60 second timeout
  );

  it(
    'should fetch markets',
    async () => {
      if (!exchange) {
        const options: ExchangeOptions = {
          key: apiConfig.apiKey,
          secret: apiConfig.secret,
          dydxMnemonic: apiConfig.secret,
          testnet: apiConfig.testnet || false,
        };

        store = new DefaultStore();
        exchange = new DydxExchange('dydx', options, store);
        await exchange.validateAccount();
      }

      const markets = await exchange.fetchMarkets();

      expect(markets).toBeDefined();
      expect(Array.isArray(markets)).toBe(true);
      expect(markets.length).toBeGreaterThan(0);

      // Check first market structure
      const market = markets[0];
      expect(market.id).toBeDefined();
      expect(market.symbol).toBeDefined();
      expect(market.base).toBeDefined();
      expect(market.quote).toBeDefined();
      expect(market.precision).toBeDefined();
      expect(market.limits).toBeDefined();
    },
    60000,
  );

  it(
    'should fetch tickers',
    async () => {
      if (!exchange) {
        const options: ExchangeOptions = {
          key: apiConfig.apiKey,
          secret: apiConfig.secret,
          dydxMnemonic: apiConfig.secret,
          testnet: apiConfig.testnet || false,
        };

        store = new DefaultStore();
        exchange = new DydxExchange('dydx', options, store);
        await exchange.validateAccount();
      }

      const tickers = await exchange.fetchTickers();

      expect(tickers).toBeDefined();
      expect(Array.isArray(tickers)).toBe(true);
      expect(tickers.length).toBeGreaterThan(0);

      // Check first ticker structure
      const ticker = tickers[0];
      expect(ticker.id).toBeDefined();
      expect(ticker.symbol).toBeDefined();
      expect(ticker.last).toBeGreaterThan(0);
    },
    60000,
  );

  it(
    'should fetch account balance',
    async () => {
      if (!exchange) {
        const options: ExchangeOptions = {
          key: apiConfig.apiKey,
          secret: apiConfig.secret,
          dydxMnemonic: apiConfig.secret,
          testnet: apiConfig.testnet || false,
        };

        store = new DefaultStore();
        exchange = new DydxExchange('dydx', options, store);
        await exchange.validateAccount();
      }

      const balance = await exchange.fetchBalance();

      expect(balance).toBeDefined();
      expect(balance.total).toBeGreaterThanOrEqual(0);
      expect(balance.free).toBeGreaterThanOrEqual(0);
      expect(balance.used).toBeGreaterThanOrEqual(0);
    },
    60000,
  );

  it(
    'should fetch positions',
    async () => {
      if (!exchange) {
        const options: ExchangeOptions = {
          key: apiConfig.apiKey,
          secret: apiConfig.secret,
          dydxMnemonic: apiConfig.secret,
          testnet: apiConfig.testnet || false,
        };

        store = new DefaultStore();
        exchange = new DydxExchange('dydx', options, store);
        await exchange.validateAccount();
      }

      const positions = await exchange.fetchPositions();

      expect(positions).toBeDefined();
      expect(Array.isArray(positions)).toBe(true);
      // Positions array may be empty if no open positions
    },
    60000,
  );

  it(
    'should fetch open orders',
    async () => {
      if (!exchange) {
        const options: ExchangeOptions = {
          key: apiConfig.apiKey,
          secret: apiConfig.secret,
          dydxMnemonic: apiConfig.secret,
          testnet: apiConfig.testnet || false,
        };

        store = new DefaultStore();
        exchange = new DydxExchange('dydx', options, store);
        await exchange.validateAccount();
      }

      const orders = await exchange.fetchOpenOrders();

      expect(orders).toBeDefined();
      expect(Array.isArray(orders)).toBe(true);
      // Orders array may be empty if no open orders
    },
    60000,
  );

  it(
    'should fetch OHLCV data',
    async () => {
      if (!exchange) {
        const options: ExchangeOptions = {
          key: apiConfig.apiKey,
          secret: apiConfig.secret,
          dydxMnemonic: apiConfig.secret,
          testnet: apiConfig.testnet || false,
        };

        store = new DefaultStore();
        exchange = new DydxExchange('dydx', options, store);
        await exchange.validateAccount();
      }

      const candles = await exchange.fetchOHLCV({ symbol: 'BTC-USD', interval: '1h', limit: 10 });

      expect(candles).toBeDefined();
      expect(Array.isArray(candles)).toBe(true);
      expect(candles.length).toBeGreaterThan(0);
      expect(candles.length).toBeLessThanOrEqual(10);

      // Check first candle structure
      const candle = candles[0];
      expect(candle.timestamp).toBeDefined();
      expect(candle.open).toBeGreaterThan(0);
      expect(candle.high).toBeGreaterThan(0);
      expect(candle.low).toBeGreaterThan(0);
      expect(candle.close).toBeGreaterThan(0);
      expect(candle.volume).toBeGreaterThanOrEqual(0);
    },
    60000,
  );

  it(
    'should start exchange and connect WebSocket',
    async () => {
      if (!exchange) {
        const options: ExchangeOptions = {
          key: apiConfig.apiKey,
          secret: apiConfig.secret,
          dydxMnemonic: apiConfig.secret,
          testnet: apiConfig.testnet || false,
        };

        store = new DefaultStore();
        exchange = new DydxExchange('dydx', options, store);
        await exchange.validateAccount();
      }

      await exchange.start();

      // Wait a bit for WebSocket to connect
      await new Promise((resolve) => setTimeout(resolve, 2000));

      expect((exchange as any).websocket).toBeDefined();
      expect(store.markets.length).toBeGreaterThan(0);
    },
    60000,
  );
});

/**
 * Instructions for running tests:
 *
 * 1. Update .apikey.json with your dYdX credentials:
 *    {
 *      "apiKey": "dydx1...",
 *      "secret": "word1 word2 word3 ...", // 12-word mnemonic
 *      "testnet": true
 *    }
 *
 * 2. Remove .skip from describe.skip above
 *
 * 3. Run tests:
 *    yarn test dydx.integration.test.ts
 *
 * NOTE: dYdX v4 SDK requires a mnemonic phrase, not just a private key.
 * If you only have a private key, you'll need to recover or generate
 * a new wallet with a mnemonic.
 */
