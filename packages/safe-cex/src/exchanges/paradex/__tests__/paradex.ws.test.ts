import { describe, it, expect } from 'vitest';
import WebSocket from 'ws';

/**
 * Test suite for Paradex WebSocket message format
 * This validates the JSON-RPC 2.0 format and channel subscriptions
 */
describe('Paradex WebSocket Format', () => {
  const WS_URL = 'wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false';
  const TEST_MARKET = 'BTC-USD-PERP';
  const TIMEOUT = 30000;

  it('should connect to Paradex public WebSocket', async () => {
    const ws = new WebSocket(WS_URL);

    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        ws.close();
        reject(new Error('WebSocket connection timeout'));
      }, 5000);

      ws.on('open', () => {
        clearTimeout(timeout);
        ws.close();
        resolve();
      });

      ws.on('error', (error) => {
        clearTimeout(timeout);
        reject(error);
      });
    });
  }, TIMEOUT);

  it('should subscribe to order_book channel with JSON-RPC 2.0 format', async () => {
    const ws = new WebSocket(WS_URL);
    const messages: any[] = [];

    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        ws.close();
        reject(new Error('Did not receive orderbook data within timeout'));
      }, 15000);

      ws.on('open', () => {
        // Subscribe using JSON-RPC 2.0 format
        const subscribeMsg = {
          jsonrpc: '2.0',
          method: 'subscribe',
          params: {
            channel: `order_book.${TEST_MARKET}.100ms`,
          },
          id: 1,
        };

        console.log('Sending subscription:', JSON.stringify(subscribeMsg));
        ws.send(JSON.stringify(subscribeMsg));
      });

      ws.on('message', (data: WebSocket.Data) => {
        const msg = JSON.parse(data.toString());
        console.log('Received message:', JSON.stringify(msg, null, 2));
        messages.push(msg);

        // Check if we got orderbook data
        if (msg.method === 'subscription' && msg.params?.channel?.startsWith('order_book')) {
          clearTimeout(timeout);
          ws.close();
          resolve();
        }
      });

      ws.on('error', (error) => {
        clearTimeout(timeout);
        reject(error);
      });
    });

    // Verify we received messages
    expect(messages.length).toBeGreaterThan(0);

    // Find orderbook subscription message
    const orderbookMsg = messages.find(m =>
      m.method === 'subscription' &&
      m.params?.channel?.startsWith('order_book')
    );

    expect(orderbookMsg).toBeDefined();
    expect(orderbookMsg?.jsonrpc).toBe('2.0');
    expect(orderbookMsg?.method).toBe('subscription');
    expect(orderbookMsg?.params?.channel).toContain('order_book');
    expect(orderbookMsg?.params?.data).toBeDefined();

    // Verify orderbook data structure
    const data = orderbookMsg?.params?.data;
    console.log('Orderbook data structure:', JSON.stringify(data, null, 2));

    // Data should have updates, inserts, or deletes
    const hasUpdates = Array.isArray(data?.updates);
    const hasInserts = Array.isArray(data?.inserts);
    const hasDeletes = Array.isArray(data?.deletes);

    expect(hasUpdates || hasInserts || hasDeletes).toBe(true);
    expect(data?.market).toBe(TEST_MARKET);
  }, TIMEOUT);

  it('should receive orderbook updates with correct price level format', async () => {
    const ws = new WebSocket(WS_URL);
    let orderbookData: any = null;

    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        ws.close();
        reject(new Error('Did not receive orderbook data within timeout'));
      }, 15000);

      ws.on('open', () => {
        const subscribeMsg = {
          jsonrpc: '2.0',
          method: 'subscribe',
          params: {
            channel: `order_book.${TEST_MARKET}.100ms`,
          },
          id: 1,
        };

        ws.send(JSON.stringify(subscribeMsg));
      });

      ws.on('message', (data: WebSocket.Data) => {
        const msg = JSON.parse(data.toString());

        if (msg.method === 'subscription' && msg.params?.channel?.startsWith('order_book')) {
          orderbookData = msg.params.data;
          clearTimeout(timeout);
          ws.close();
          resolve();
        }
      });

      ws.on('error', (error) => {
        clearTimeout(timeout);
        reject(error);
      });
    });

    expect(orderbookData).toBeDefined();

    // Check for price levels in updates/inserts
    const priceLevels = [
      ...(orderbookData.updates || []),
      ...(orderbookData.inserts || []),
    ];

    if (priceLevels.length > 0) {
      const firstLevel = priceLevels[0];
      console.log('First price level:', JSON.stringify(firstLevel, null, 2));

      expect(firstLevel).toHaveProperty('price');
      expect(firstLevel).toHaveProperty('side');
      expect(firstLevel).toHaveProperty('size');
      expect(['BUY', 'SELL']).toContain(firstLevel.side);
    }
  }, TIMEOUT);

  it('should handle invalid subscription format', async () => {
    const ws = new WebSocket(WS_URL);
    let errorResponse: any = null;

    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        ws.close();
        resolve(); // Don't fail if no error received
      }, 5000);

      ws.on('open', () => {
        // Send invalid format (old format we were using)
        const invalidMsg = {
          type: 'subscribe',
          channel: 'orderbook',
          market: TEST_MARKET,
          depth: 20,
        };

        console.log('Sending invalid subscription:', JSON.stringify(invalidMsg));
        ws.send(JSON.stringify(invalidMsg));
      });

      ws.on('message', (data: WebSocket.Data) => {
        const msg = JSON.parse(data.toString());
        console.log('Received message:', JSON.stringify(msg, null, 2));

        if (msg.error) {
          errorResponse = msg;
          clearTimeout(timeout);
          ws.close();
          resolve();
        }
      });

      ws.on('error', (error) => {
        clearTimeout(timeout);
        reject(error);
      });
    });

    // Should receive error response
    expect(errorResponse).toBeDefined();
    expect(errorResponse?.error?.code).toBe(-32600);
    expect(errorResponse?.error?.message).toContain('invalid request');
  }, TIMEOUT);
});
