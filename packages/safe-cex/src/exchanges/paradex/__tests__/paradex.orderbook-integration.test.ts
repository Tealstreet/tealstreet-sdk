import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { DefaultStore } from '../../../store/store.base';
import { OrderBook } from '../../../types';
import { ParadexExchange } from '../paradex.exchange';

/**
 * Integration test for Paradex orderbook WebSocket
 * Tests the full flow: connect -> subscribe -> receive data -> callback
 */
describe('Paradex OrderBook WebSocket Integration', () => {
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

    // Wait a bit for WebSocket to connect
    await new Promise((resolve) => setTimeout(resolve, 500));
  });

  afterAll(() => {
    if (exchange) {
      exchange.dispose();
    }
  });

  it('should receive orderbook updates via listenOrderBook', async () => {
    const TEST_SYMBOL = 'BTC-USD-PERP';
    const receivedOrderbooks: OrderBook[] = [];

    // Set up listener
    const unsubscribe = exchange.listenOrderBook(TEST_SYMBOL, (orderbook) => {
      receivedOrderbooks.push(orderbook);
    });

    // Wait for orderbook data (Paradex sends snapshots every 100ms)
    await new Promise((resolve) => setTimeout(resolve, 2000));

    // Cleanup
    unsubscribe();

    // Verify we received data
    expect(receivedOrderbooks.length).toBeGreaterThan(0);

    // Verify orderbook structure
    const firstOrderbook = receivedOrderbooks[0];
    expect(firstOrderbook).toBeDefined();
    expect(firstOrderbook.bids).toBeDefined();
    expect(firstOrderbook.asks).toBeDefined();
    expect(Array.isArray(firstOrderbook.bids)).toBe(true);
    expect(Array.isArray(firstOrderbook.asks)).toBe(true);

    // Verify we have bid and ask data
    expect(firstOrderbook.bids.length).toBeGreaterThan(0);
    expect(firstOrderbook.asks.length).toBeGreaterThan(0);

    // Verify price level structure
    const firstBid = firstOrderbook.bids[0];
    expect(firstBid).toHaveProperty('price');
    expect(firstBid).toHaveProperty('amount');
    expect(firstBid).toHaveProperty('total');
    expect(typeof firstBid.price).toBe('number');
    expect(typeof firstBid.amount).toBe('number');
    expect(typeof firstBid.total).toBe('number');

    const firstAsk = firstOrderbook.asks[0];
    expect(firstAsk).toHaveProperty('price');
    expect(firstAsk).toHaveProperty('amount');
    expect(firstAsk).toHaveProperty('total');

    console.log(`Received ${receivedOrderbooks.length} orderbook updates`);
    console.log(`First orderbook - Bids: ${firstOrderbook.bids.length}, Asks: ${firstOrderbook.asks.length}`);
    console.log(`Best bid: ${firstBid.price}, Best ask: ${firstAsk.price}, Spread: ${(firstAsk.price - firstBid.price).toFixed(2)}`);
  }, 10000);

  it('should handle multiple listeners on same symbol', async () => {
    const TEST_SYMBOL = 'ETH-USD-PERP';
    const receivedOrderbooks1: OrderBook[] = [];
    const receivedOrderbooks2: OrderBook[] = [];

    // Set up two listeners on same symbol
    const unsubscribe1 = exchange.listenOrderBook(TEST_SYMBOL, (orderbook) => {
      receivedOrderbooks1.push(orderbook);
    });

    const unsubscribe2 = exchange.listenOrderBook(TEST_SYMBOL, (orderbook) => {
      receivedOrderbooks2.push(orderbook);
    });

    // Wait for data
    await new Promise((resolve) => setTimeout(resolve, 1500));

    // Cleanup
    unsubscribe1();
    unsubscribe2();

    // Both listeners should receive the same number of updates
    expect(receivedOrderbooks1.length).toBeGreaterThan(0);
    expect(receivedOrderbooks2.length).toBe(receivedOrderbooks1.length);

    console.log(`Multiple listeners test - Listener 1: ${receivedOrderbooks1.length}, Listener 2: ${receivedOrderbooks2.length} updates`);
  }, 10000);

  it('should stop receiving updates after unsubscribe', async () => {
    const TEST_SYMBOL = 'SOL-USD-PERP';
    const receivedOrderbooks: OrderBook[] = [];

    // Set up listener
    const unsubscribe = exchange.listenOrderBook(TEST_SYMBOL, (orderbook) => {
      receivedOrderbooks.push(orderbook);
    });

    // Wait for some updates
    await new Promise((resolve) => setTimeout(resolve, 500));
    const countBeforeUnsubscribe = receivedOrderbooks.length;

    // Unsubscribe
    unsubscribe();

    // Wait more time
    await new Promise((resolve) => setTimeout(resolve, 1000));
    const countAfterUnsubscribe = receivedOrderbooks.length;

    // Count should not increase significantly after unsubscribe
    // (might have 1-2 more due to throttling/timing)
    expect(countAfterUnsubscribe - countBeforeUnsubscribe).toBeLessThan(3);

    console.log(`Unsubscribe test - Before: ${countBeforeUnsubscribe}, After: ${countAfterUnsubscribe}`);
  }, 10000);
});
