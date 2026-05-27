// @vitest-environment node
import { readFileSync } from 'fs';
import { join } from 'path';

import * as ed25519 from '@noble/ed25519';
import { sha512 } from '@noble/hashes/sha2.js';
import axios, { AxiosInstance } from 'axios';
import { beforeAll, describe, expect, it } from 'vitest';

const sha512ForEd25519: NonNullable<typeof ed25519.hashes.sha512> = (message) =>
  Uint8Array.from(sha512(message));

// Set the crypto hash function for noble/ed25519 v3
ed25519.hashes.sha512 = sha512ForEd25519;

interface ApiKeyConfig {
  exchange: string;
  testnet: boolean;
  name: string;
  apiKey: string;
  secret: string;
  user: string;
  active: boolean;
  color: string;
}

const BASE_URL = 'https://api.backpack.exchange';

/**
 * Generate ED25519 signature for Backpack API requests
 */
async function generateSignature(
  instruction: string,
  params: Record<string, any>,
  timestamp: number,
  window: number,
  privateKeyBase64: string,
): Promise<string> {
  const sortedKeys = Object.keys(params).sort();
  const queryParts: string[] = [];

  for (const key of sortedKeys) {
    const value = params[key];
    if (value !== null && value !== undefined) {
      queryParts.push(`${key}=${encodeURIComponent(String(value))}`);
    }
  }

  const queryString = queryParts.join('&');
  const signaturePayload = `instruction=${instruction}${queryString ? '&' + queryString : ''}&timestamp=${timestamp}&window=${window}`;
  const messageBytes = new TextEncoder().encode(signaturePayload);
  const privateKeyBytes = Uint8Array.from(Buffer.from(privateKeyBase64, 'base64'));
  const signatureBytes = await ed25519.signAsync(messageBytes, privateKeyBytes);

  return Buffer.from(signatureBytes).toString('base64');
}

/**
 * Create authenticated headers for API requests
 */
async function createAuthHeaders(
  instruction: string,
  params: Record<string, any>,
  apiKey: string,
  secret: string,
  window: number = 5000,
): Promise<Record<string, string>> {
  const timestamp = Date.now();
  const signature = await generateSignature(instruction, params, timestamp, window, secret);

  return {
    'X-API-Key': apiKey,
    'X-Signature': signature,
    'X-Timestamp': timestamp.toString(),
    'X-Window': window.toString(),
    'Content-Type': 'application/json',
  };
}

describe('Backpack REST API Integration', () => {
  let apiConfig: ApiKeyConfig;
  let client: AxiosInstance;

  beforeAll(() => {
    const configPath = join(__dirname, '..', '.apikey.json');
    const configData = readFileSync(configPath, 'utf-8');
    apiConfig = JSON.parse(configData);

    client = axios.create({
      baseURL: BASE_URL,
      timeout: 10000,
    });
  });

  describe('Public Endpoints', () => {
    it('should fetch server time', async () => {
      const response = await client.get('/api/v1/time');

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(response.data.serverTime).toBeDefined();
      expect(typeof response.data.serverTime).toBe('number');
    });

    it('should fetch system status', async () => {
      const response = await client.get('/api/v1/status');

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(response.data.status).toBeDefined();
    });

    it('should fetch all markets', async () => {
      const response = await client.get('/api/v1/markets');

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(Array.isArray(response.data)).toBe(true);
      expect(response.data.length).toBeGreaterThan(0);

      const market = response.data[0];
      expect(market.symbol).toBeDefined();
      expect(market.baseSymbol).toBeDefined();
      expect(market.quoteSymbol).toBeDefined();
    });

    it('should fetch assets', async () => {
      const response = await client.get('/api/v1/assets');

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(Array.isArray(response.data)).toBe(true);
      expect(response.data.length).toBeGreaterThan(0);
    });

    it('should fetch ticker for SOL_USDC', async () => {
      const response = await client.get('/api/v1/ticker', {
        params: { symbol: 'SOL_USDC' },
      });

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(response.data.symbol).toBe('SOL_USDC');
      expect(response.data.lastPrice).toBeDefined();
      expect(response.data.volume).toBeDefined();
    });

    it('should fetch all tickers', async () => {
      const response = await client.get('/api/v1/tickers');

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(Array.isArray(response.data)).toBe(true);
      expect(response.data.length).toBeGreaterThan(0);

      const ticker = response.data[0];
      expect(ticker.symbol).toBeDefined();
      expect(ticker.lastPrice).toBeDefined();
    });

    it('should fetch order book depth for SOL_USDC', async () => {
      const response = await client.get('/api/v1/depth', {
        params: { symbol: 'SOL_USDC' },
      });

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(response.data.bids).toBeDefined();
      expect(response.data.asks).toBeDefined();
      expect(Array.isArray(response.data.bids)).toBe(true);
      expect(Array.isArray(response.data.asks)).toBe(true);

      if (response.data.bids.length > 0) {
        const bid = response.data.bids[0];
        expect(Array.isArray(bid)).toBe(true);
        expect(bid.length).toBe(2); // [price, quantity]
      }
    });

    it('should fetch recent trades for SOL_USDC', async () => {
      const response = await client.get('/api/v1/trades', {
        params: { symbol: 'SOL_USDC' },
      });

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(Array.isArray(response.data)).toBe(true);

      if (response.data.length > 0) {
        const trade = response.data[0];
        expect(trade.price).toBeDefined();
        expect(trade.quantity).toBeDefined();
        expect(trade.timestamp).toBeDefined();
        expect(trade.side).toBeDefined();
      }
    });

    it('should fetch klines/candles for SOL_USDC', async () => {
      const response = await client.get('/api/v1/klines', {
        params: {
          symbol: 'SOL_USDC',
          interval: '1m',
        },
      });

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(Array.isArray(response.data)).toBe(true);

      if (response.data.length > 0) {
        const kline = response.data[0];
        expect(kline.start).toBeDefined();
        expect(kline.open).toBeDefined();
        expect(kline.high).toBeDefined();
        expect(kline.low).toBeDefined();
        expect(kline.close).toBeDefined();
        expect(kline.volume).toBeDefined();
      }
    });
  });

  describe('Authenticated Endpoints', () => {
    it('should fetch account balances', async () => {
      const instruction = 'balanceQuery';
      const params = {};

      const headers = await createAuthHeaders(instruction, params, apiConfig.apiKey, apiConfig.secret);

      const response = await client.get('/api/v1/capital', { headers });

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(Array.isArray(response.data)).toBe(true);

      if (response.data.length > 0) {
        const balance = response.data[0];
        expect(balance.asset).toBeDefined();
        expect(balance.available).toBeDefined();
        expect(balance.locked).toBeDefined();
      }
    });

    it('should fetch open orders', async () => {
      const instruction = 'orderQueryAll';
      const params = { symbol: 'SOL_USDC' };

      const headers = await createAuthHeaders(instruction, params, apiConfig.apiKey, apiConfig.secret);

      const response = await client.get('/api/v1/orders', {
        headers,
        params,
      });

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(Array.isArray(response.data)).toBe(true);

      if (response.data.length > 0) {
        const order = response.data[0];
        expect(order.id).toBeDefined();
        expect(order.symbol).toBeDefined();
        expect(order.side).toBeDefined();
        expect(order.orderType).toBeDefined();
        expect(order.price).toBeDefined();
        expect(order.quantity).toBeDefined();
        expect(order.status).toBeDefined();
      }
    });

    it('should fetch fill history', async () => {
      const instruction = 'fillHistoryQueryAll';
      const params = { symbol: 'SOL_USDC' };

      const headers = await createAuthHeaders(instruction, params, apiConfig.apiKey, apiConfig.secret);

      const response = await client.get('/history/fills', {
        headers,
        params,
      });

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(Array.isArray(response.data)).toBe(true);

      if (response.data.length > 0) {
        const fill = response.data[0];
        expect(fill.id).toBeDefined();
        expect(fill.orderId).toBeDefined();
        expect(fill.symbol).toBeDefined();
        expect(fill.side).toBeDefined();
        expect(fill.price).toBeDefined();
        expect(fill.quantity).toBeDefined();
        expect(fill.timestamp).toBeDefined();
      }
    });

    it('should fetch order history', async () => {
      const instruction = 'orderHistoryQueryAll';
      const params = { symbol: 'SOL_USDC' };

      const headers = await createAuthHeaders(instruction, params, apiConfig.apiKey, apiConfig.secret);

      const response = await client.get('/history/orders', {
        headers,
        params,
      });

      expect(response.status).toBe(200);
      expect(response.data).toBeDefined();
      expect(Array.isArray(response.data)).toBe(true);

      if (response.data.length > 0) {
        const order = response.data[0];
        expect(order.id).toBeDefined();
        expect(order.symbol).toBeDefined();
        expect(order.side).toBeDefined();
        expect(order.status).toBeDefined();
      }
    });

    it('should place and cancel a test order', async () => {
      // First, get current market price
      const tickerResponse = await client.get('/api/v1/ticker', {
        params: { symbol: 'SOL_USDC' },
      });

      const lastPrice = parseFloat(tickerResponse.data.lastPrice);
      const testPrice = (lastPrice * 0.5).toFixed(2); // 50% below market to avoid execution

      // Place order
      const placeInstruction = 'orderExecute';
      const placeParams = {
        orderType: 'Limit',
        quantity: '0.1',
        price: testPrice,
        side: 'Bid',
        symbol: 'SOL_USDC',
        timeInForce: 'GTC',
      };

      const placeHeaders = await createAuthHeaders(placeInstruction, placeParams, apiConfig.apiKey, apiConfig.secret);

      const placeResponse = await client.post('/api/v1/order', placeParams, {
        headers: placeHeaders,
      });

      expect(placeResponse.status).toBe(200);
      expect(placeResponse.data).toBeDefined();
      expect(placeResponse.data.id).toBeDefined();

      const orderId = placeResponse.data.id;

      // Wait a bit for order to be processed
      await new Promise((resolve) => setTimeout(resolve, 1000));

      // Cancel the order
      const cancelInstruction = 'orderCancel';
      const cancelParams = {
        orderId: orderId.toString(),
        symbol: 'SOL_USDC',
      };

      const cancelHeaders = await createAuthHeaders(
        cancelInstruction,
        cancelParams,
        apiConfig.apiKey,
        apiConfig.secret,
      );

      const cancelResponse = await client.delete('/api/v1/order', {
        headers: cancelHeaders,
        params: cancelParams,
      });

      expect(cancelResponse.status).toBe(200);
      expect(cancelResponse.data).toBeDefined();
    }, 15000);
  });
});
