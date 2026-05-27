import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { DefaultStore } from '../../../store/store.base';
import { ParadexExchange } from '../paradex.exchange';
import { Ticker } from '../../../types';

/**
 * Integration test for Paradex WebSocket
 * Tests the full flow with detailed debugging
 */
describe('Paradex WebSocket Integration', () => {
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

  it('should connect to public WebSocket', async () => {
    const wsPublic = (exchange as any).publicWebsocket;
    expect(wsPublic).toBeDefined();

    // Check if WebSocket is connected
    const isConnected = wsPublic.isConnected;
    console.log('WebSocket connected:', isConnected);
    expect(isConnected).toBe(true);
  }, 10000);

  it('should receive orderbook data via listenOrderBook', async () => {
    const TEST_SYMBOL = 'BTC-USD-PERP';
    const receivedData: any[] = [];
    let callbackCount = 0;

    // Set up listener
    console.log(`Setting up listener for ${TEST_SYMBOL}`);
    const unsubscribe = exchange.listenOrderBook(TEST_SYMBOL, (orderbook) => {
      callbackCount++;
      receivedData.push(orderbook);
      console.log(`Callback ${callbackCount}: Received orderbook - Bids: ${orderbook.bids?.length || 0}, Asks: ${orderbook.asks?.length || 0}`);
    });

    // Wait for data
    console.log('Waiting for orderbook data...');
    await new Promise((resolve) => setTimeout(resolve, 5000));

    // Cleanup
    unsubscribe();

    console.log(`Total callbacks received: ${callbackCount}`);
    console.log(`Total orderbooks stored: ${receivedData.length}`);

    // Check store for ticker data
    const ticker = store.tickers.find((t: Ticker) => t.symbol === TEST_SYMBOL);
    console.log('Ticker in store:', ticker);

    // Verify we received data (even if empty orderbook)
    // The key is that the callback was called
    expect(receivedData.length).toBeGreaterThan(0);

    if (receivedData.length > 0) {
      const firstOrderbook = receivedData[0];
      console.log('First orderbook structure:', {
        hasBids: !!firstOrderbook.bids,
        hasAsks: !!firstOrderbook.asks,
        bidsLength: firstOrderbook.bids?.length,
        asksLength: firstOrderbook.asks?.length,
        firstBid: firstOrderbook.bids?.[0],
        firstAsk: firstOrderbook.asks?.[0],
      });

      expect(firstOrderbook.bids).toBeDefined();
      expect(firstOrderbook.asks).toBeDefined();
      // Don't require bid/ask length > 0 as orderbook might be empty on first snapshot
    }
  }, 15000);

  it('should receive BBO/ticker updates', async () => {
    const TEST_SYMBOL = 'BTC-USD-PERP';

    // Wait a bit for ticker updates
    await new Promise((resolve) => setTimeout(resolve, 2000));

    // Check store for ticker
    const ticker = store.tickers.find((t: Ticker) => t.symbol === TEST_SYMBOL);
    console.log('Ticker data:', ticker);

    expect(ticker).toBeDefined();
    if (ticker) {
      expect(ticker.bid).toBeGreaterThan(0);
      expect(ticker.ask).toBeGreaterThan(0);
      expect(ticker.last).toBeGreaterThan(0);
      console.log(`BBO - Bid: ${ticker.bid}, Ask: ${ticker.ask}, Last: ${ticker.last}`);
    }
  }, 10000);

  it('should maintain WebSocket connection health', async () => {
    const wsPublic = (exchange as any).publicWebsocket;

    // Wait for messages
    await new Promise((resolve) => setTimeout(resolve, 2000));

    // Check that WebSocket is still connected
    expect(wsPublic.isConnected).toBe(true);

    // Check that we have active subscriptions
    const messageHandlers = wsPublic.messageHandlers;
    console.log('Message handlers:', Object.keys(messageHandlers));
    expect(messageHandlers).toBeDefined();
    expect(Object.keys(messageHandlers).length).toBeGreaterThan(0);

    // Verify we have data in the store
    const tickers = store.tickers;
    console.log(`Store has ${tickers.length} tickers`);
    expect(tickers.length).toBeGreaterThan(0);
  }, 10000);
});
