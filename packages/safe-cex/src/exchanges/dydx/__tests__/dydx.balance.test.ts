import { describe, it, expect, beforeAll, vi } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import { DydxExchange } from '../dydx.exchange';
import { ExchangeOptions } from '../../../types';
import { DefaultStore } from '../../../store/store.base';

/**
 * Balance fetch test for dYdX v4 Exchange
 *
 * This test validates that the dYdX integration can properly fetch user balance
 * using the credentials from .apikey.json
 */

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet?: boolean;
}

describe('DydxExchange - Balance Fetch Test', () => {
  let apiConfig: ApiKeyConfig;
  let exchange: DydxExchange;
  let store: DefaultStore;

  beforeAll(async () => {
    // Replace mocked fetch with real fetch for dYdX SDK network calls
    // Try undici first (built-in Node.js fetch), then fall back to node-fetch
    try {
      const undici = await import('undici');
      global.fetch = undici.fetch as any;
    } catch {
      try {
        // @ts-ignore - node-fetch might not have types
        const nodeFetch = await import('node-fetch');
        // @ts-ignore
        global.fetch = nodeFetch.default as any;
      } catch {
        // If neither is available, use native fetch if available (Node 18+)
        if (typeof fetch !== 'undefined') {
          global.fetch = fetch;
        }
      }
    }

    // Load API credentials from .apikey.json
    const configPath = join(__dirname, '..', '.apikey.json');
    try {
      const configData = readFileSync(configPath, 'utf-8');
      apiConfig = JSON.parse(configData);
    } catch (error) {
      throw error;
    }
  });

  it('should load API credentials correctly', () => {
    expect(apiConfig).toBeDefined();
    expect(apiConfig.apiKey).toBeTruthy();
    expect(apiConfig.secret).toBeTruthy();
  });

  it(
    'should initialize wallet from private key',
    async () => {
      const { LocalWallet } = await import('@dydxprotocol/v4-client-js');

      const privateKey = apiConfig.secret.startsWith('0x')
        ? apiConfig.secret.slice(2)
        : apiConfig.secret;

      const wallet = await LocalWallet.fromPrivateKey(privateKey, 'dydx');

      expect(wallet).toBeDefined();
      expect(wallet.address).toBe(apiConfig.apiKey);
    },
    30000,
  );

  it(
    'should create exchange instance and fetch balance',
    async () => {
      // Create exchange options
      const options: ExchangeOptions = {
        key: apiConfig.apiKey,
        secret: apiConfig.secret,
        // Try using secret as mnemonic if it's a phrase, otherwise leave undefined
        dydxMnemonic: apiConfig.secret.includes(' ') ? apiConfig.secret : undefined,
        testnet: apiConfig.testnet || false,
      };

      console.log('Creating exchange with options:', {
        key: options.key,
        hasMnemonic: !!options.dydxMnemonic,
        testnet: options.testnet,
      });

      // Create store and exchange
      store = new DefaultStore();
      exchange = new DydxExchange('dydx', options, store);

      expect(exchange).toBeDefined();

      // Validate account (initializes SDK)
      console.log('Validating account...');
      const validationResult = await exchange.validateAccount();

      if (validationResult) {
        console.error('Validation failed:', validationResult);
        throw new Error(`Account validation failed: ${validationResult}`);
      }

      console.log('Account validated successfully');

      // Fetch balance
      console.log('Fetching balance...');
      const balance = await exchange.fetchBalance();

      console.log('Balance fetched:', balance);

      expect(balance).toBeDefined();
      expect(typeof balance.total).toBe('number');
      expect(typeof balance.free).toBe('number');
      expect(typeof balance.used).toBe('number');
      expect(balance.total).toBeGreaterThanOrEqual(0);
      expect(balance.free).toBeGreaterThanOrEqual(0);
      expect(balance.used).toBeGreaterThanOrEqual(0);

      // Fetch positions
      console.log('Fetching positions...');
      const positions = await exchange.fetchPositions();

      console.log('Positions fetched:', positions);

      expect(positions).toBeDefined();
      expect(Array.isArray(positions)).toBe(true);
      console.log('Number of positions:', positions.length);

      // Fetch open orders
      console.log('Fetching open orders...');
      const orders = await exchange.fetchOpenOrders();

      console.log('Open orders fetched:', orders);

      expect(orders).toBeDefined();
      expect(Array.isArray(orders)).toBe(true);
      console.log('Number of open orders:', orders.length);
    },
    60000, // 60 second timeout
  );
});
