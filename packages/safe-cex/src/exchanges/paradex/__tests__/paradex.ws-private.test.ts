import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import { DefaultStore } from '../../../store/store.base';
import { ParadexExchange } from '../paradex.exchange';

/**
 * Integration test for Paradex private WebSocket authentication
 * Tests the full flow: connect -> authenticate -> receive confirmation
 *
 * Uses credentials from .apikey.json in the paradex directory
 */
describe('Paradex Private WebSocket Authentication', () => {
  const TIMEOUT = 30000;
  let exchange: ParadexExchange;
  let store: DefaultStore;
  let apiKey: string | null = null;

  beforeAll(() => {
    // Load API key from .apikey.json
    try {
      const apiKeyPath = join(__dirname, '.apikey.json');
      const apiKeyData = JSON.parse(readFileSync(apiKeyPath, 'utf-8'));
      apiKey = apiKeyData.apiKey;
      console.log('Loaded API key from .apikey.json');
    } catch (error) {
      console.warn('Could not load .apikey.json - skipping private WebSocket tests');
    }
  });

  it('should connect and authenticate to private WebSocket', async () => {
    if (!apiKey) {
      console.log('Skipping test - API key not loaded');
      return;
    }

    store = new DefaultStore();

    // Create exchange instance with auth credentials
    exchange = new ParadexExchange(
      'paradex',
      {
        testnet: false,
        key: apiKey, // Ethereum private key
        secret: '', // Not used for Paradex
      },
      store
    );

    // Track authentication status
    let authenticated = false;
    let authError: string | null = null;

    // Create a promise that resolves when we get authentication confirmation
    const authPromise = new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        cleanup();
        reject(new Error('Authentication timeout - no confirmation received within 15s'));
      }, 15000);

      // Monitor WebSocket for authentication
      const checkAuth = () => {
        // Check if private WebSocket is authenticated
        const privateWs = (exchange as any).wsPrivate;
        if (privateWs && (privateWs as any).authenticated) {
          authenticated = true;
          clearTimeout(timeout);
          cleanup();
          resolve();
        }
      };

      // Poll for authentication status
      const pollInterval = setInterval(checkAuth, 100);

      const cleanup = () => {
        clearInterval(pollInterval);
      };
    });

    try {
      // Start the exchange (initializes SDK and connects WebSocket)
      await exchange.start();

      // Wait for authentication
      await authPromise;

      // Verify authentication succeeded
      expect(authenticated).toBe(true);
      expect(authError).toBeNull();

      console.log('✓ Private WebSocket authenticated successfully');
    } finally {
      // Cleanup
      await exchange.dispose();
    }
  }, TIMEOUT);

  it('should handle authentication errors gracefully', async () => {
    const invalidKey = '0x0000000000000000000000000000000000000000000000000000000000000001';

    store = new DefaultStore();

    // Create exchange with invalid credentials
    exchange = new ParadexExchange(
      'paradex',
      {
        testnet: false,
        key: invalidKey,
        secret: '',
      },
      store
    );

    let errorCaught = false;

    try {
      // Start should fail or timeout
      await Promise.race([
        exchange.start(),
        new Promise((_, reject) =>
          setTimeout(() => reject(new Error('Expected auth to fail')), 5000)
        ),
      ]);
    } catch (error) {
      errorCaught = true;
      console.log('✓ Authentication correctly failed for invalid credentials');
    } finally {
      await exchange.dispose();
    }

    // We expect either an error or timeout
    expect(errorCaught).toBe(true);
  }, TIMEOUT);

  it('should send subscriptions after authentication', async () => {
    if (!apiKey) {
      console.log('Skipping test - API key not loaded');
      return;
    }

    store = new DefaultStore();

    exchange = new ParadexExchange(
      'paradex',
      {
        testnet: false,
        key: apiKey,
        secret: '',
      },
      store
    );

    // Track messages sent
    const sentMessages: any[] = [];

    // Create a promise that resolves when we see subscription messages
    const subscriptionPromise = new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        cleanup();
        reject(new Error('Did not see subscription messages within 15s'));
      }, 15000);

      // Intercept WebSocket send to monitor messages
      const originalSend = WebSocket.prototype.send;
      WebSocket.prototype.send = function (data: any) {
        try {
          const msg = JSON.parse(data);
          sentMessages.push(msg);
          console.log('Sent message:', JSON.stringify(msg, null, 2));

          // Check if we've sent subscription messages
          const hasOrdersSub = sentMessages.some(
            m => m.method === 'subscribe' && m.params?.channel === 'orders'
          );
          const hasPositionsSub = sentMessages.some(
            m => m.method === 'subscribe' && m.params?.channel === 'positions'
          );
          const hasBalancesSub = sentMessages.some(
            m => m.method === 'subscribe' && m.params?.channel === 'balances'
          );

          if (hasOrdersSub && hasPositionsSub && hasBalancesSub) {
            clearTimeout(timeout);
            cleanup();
            WebSocket.prototype.send = originalSend;
            resolve();
          }
        } catch (e) {
          // Not JSON
        }
        return originalSend.call(this, data);
      };

      const cleanup = () => {
        WebSocket.prototype.send = originalSend;
      };
    });

    try {
      await exchange.start();
      await subscriptionPromise;

      // Verify we sent auth and subscriptions
      const authMsg = sentMessages.find(m => m.method === 'auth');
      expect(authMsg).toBeDefined();
      expect(authMsg?.jsonrpc).toBe('2.0');
      expect(authMsg?.params?.bearer).toBeDefined();

      const ordersSub = sentMessages.find(
        m => m.method === 'subscribe' && m.params?.channel === 'orders'
      );
      expect(ordersSub).toBeDefined();
      expect(ordersSub?.jsonrpc).toBe('2.0');

      console.log('✓ Sent auth and subscription messages in correct JSON-RPC 2.0 format');
    } finally {
      await exchange.dispose();
    }
  }, TIMEOUT);
});
