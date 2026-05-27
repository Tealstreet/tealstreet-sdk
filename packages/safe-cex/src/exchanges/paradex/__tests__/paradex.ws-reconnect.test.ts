import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { DefaultStore } from '../../../store/store.base';
import { ParadexExchange } from '../paradex.exchange';

/**
 * Integration test for Paradex WebSocket reconnection
 * Tests that orderbook subscriptions are properly restored after reconnect
 */
describe('Paradex WebSocket Reconnection', () => {
  let exchange: ParadexExchange;
  let store: DefaultStore;

  beforeAll(async () => {
    store = new DefaultStore();

    // Create exchange instance (no auth needed for public WebSocket)
    exchange = new ParadexExchange(
      'paradex',
      { key: '', secret: '', testnet: false },
      store
    );

    // Start exchange (initializes WebSocket connection)
    await exchange.start();

    // Wait for WebSocket to connect and subscribe
    await new Promise((resolve) => setTimeout(resolve, 2000));
  });

  afterAll(() => {
    if (exchange) {
      exchange.dispose();
    }
  });

  it('should resubscribe to orderbooks when onOpen is called', async () => {
    const TEST_SYMBOL = 'BTC-USD-PERP';

    // Set up listener
    console.log(`Setting up listener for ${TEST_SYMBOL}`);
    const unsubscribe = exchange.listenOrderBook(TEST_SYMBOL, () => {
      // Callback doesn't matter for this test
    });

    // Get WebSocket and check it has the callback registered
    const wsPublic = (exchange as any).publicWebsocket;
    const callbacks = (wsPublic as any).orderbookCallbacks;
    expect(callbacks.has(TEST_SYMBOL)).toBe(true);
    expect(callbacks.get(TEST_SYMBOL).size).toBe(1);
    console.log(`Orderbook callbacks registered for ${TEST_SYMBOL}:`, callbacks.get(TEST_SYMBOL).size);

    // Track subscription messages
    const sentMessages: any[] = [];
    const originalSend = wsPublic.send.bind(wsPublic);
    wsPublic.send = (data: string) => {
      const msg = JSON.parse(data);
      sentMessages.push(msg);
      console.log('Sent message:', msg);
      return originalSend(data);
    };

    // Manually trigger onOpen to simulate reconnection
    console.log('Simulating reconnection by calling onOpen...');
    wsPublic.onOpen();

    // Wait a bit for subscriptions to be sent
    await new Promise((resolve) => setTimeout(resolve, 500));

    // Restore original send
    wsPublic.send = originalSend;

    // Check that orderbook subscription was sent
    const orderbookSubscriptions = sentMessages.filter(
      msg => msg.method === 'subscribe' && msg.params?.channel?.includes('order_book') && msg.params.channel.includes(TEST_SYMBOL)
    );

    console.log(`Found ${orderbookSubscriptions.length} orderbook subscription messages for ${TEST_SYMBOL}`);
    expect(orderbookSubscriptions.length).toBeGreaterThan(0);

    // Cleanup
    unsubscribe();
    console.log('✓ Orderbook resubscription on onOpen works correctly');
  }, 30000);

  it('should not lose callbacks map after reconnection', async () => {
    const TEST_SYMBOLS = ['ETH-USD-PERP', 'SOL-USD-PERP'];
    const unsubscribeFns: Array<() => void> = [];
    const receivedCounts = new Map<string, number>();

    // Set up listeners for multiple symbols
    TEST_SYMBOLS.forEach(symbol => {
      receivedCounts.set(symbol, 0);
      const unsubscribe = exchange.listenOrderBook(symbol, () => {
        receivedCounts.set(symbol, receivedCounts.get(symbol)! + 1);
      });
      unsubscribeFns.push(unsubscribe);
    });

    // Wait for initial data
    await new Promise((resolve) => setTimeout(resolve, 2000));

    // Get WebSocket
    const wsPublic = (exchange as any).publicWebsocket;
    const callbacks = (wsPublic as any).orderbookCallbacks;

    // Check all symbols have callbacks
    TEST_SYMBOLS.forEach(symbol => {
      expect(callbacks.has(symbol)).toBe(true);
      expect(callbacks.get(symbol).size).toBe(1);
    });
    console.log(`All ${TEST_SYMBOLS.length} symbols have callbacks registered`);

    // Simulate reconnection
    console.log('Simulating WebSocket reconnection...');
    wsPublic.ws.close();
    await new Promise((resolve) => setTimeout(resolve, 3000));

    // Check callbacks are still there
    TEST_SYMBOLS.forEach(symbol => {
      expect(callbacks.has(symbol)).toBe(true);
      expect(callbacks.get(symbol).size).toBe(1);
    });
    console.log(`All ${TEST_SYMBOLS.length} symbols still have callbacks after reconnect`);

    // Cleanup
    unsubscribeFns.forEach(fn => fn());
    console.log('✓ Multiple orderbook callbacks preserved after reconnect');
  }, 30000);
});
