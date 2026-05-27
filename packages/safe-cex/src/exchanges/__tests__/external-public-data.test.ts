import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach, vi } from 'vitest';
import { WebSocketServer, WebSocket as WsWebSocket } from 'ws';
import { ExternalPublicDataProvider, GatewayMessage } from '../external-public-data';
import { DefaultStore } from '../../store/store.base';
import { BaseExchange } from '../base';
import type { ExchangeOptions, Ticker, OrderBook, Candle, Market } from '../../types';

// Mock gateway server for testing
class MockGatewayServer {
  private wss: WebSocketServer;
  private clients = new Set<WsWebSocket>();
  private port: number;

  constructor(port: number) {
    this.port = port;
    this.wss = new WebSocketServer({ port });

    this.wss.on('connection', (ws) => {
      this.clients.add(ws);

      ws.on('message', (data) => {
        const msg = JSON.parse(data.toString());
        this.handleMessage(ws, msg);
      });

      ws.on('close', () => {
        this.clients.delete(ws);
      });
    });
  }

  private handleMessage(ws: WsWebSocket, msg: any) {
    // Echo back subscription confirmations
    if (msg.type === 'subscribe') {
      ws.send(
        JSON.stringify({
          type: 'subscribed',
          exchangeId: msg.exchangeId,
          channel: msg.channel,
          symbol: msg.symbol,
        }),
      );
    }
  }

  // Broadcast data to all connected clients
  broadcast(message: GatewayMessage) {
    const data = JSON.stringify(message);
    for (const client of this.clients) {
      if (client.readyState === WsWebSocket.OPEN) {
        client.send(data);
      }
    }
  }

  // Send tickers update
  sendTickers(exchangeId: string, tickers: Ticker[]) {
    this.broadcast({
      type: 'data',
      exchangeId,
      channel: 'tickers',
      data: tickers,
    });
  }

  // Send orderbook update
  sendOrderbook(exchangeId: string, symbol: string, orderbook: OrderBook) {
    this.broadcast({
      type: 'data',
      exchangeId,
      channel: 'orderbook',
      symbol,
      data: orderbook,
    });
  }

  // Send candle update
  sendCandle(exchangeId: string, symbol: string, interval: string, candle: Candle) {
    this.broadcast({
      type: 'data',
      exchangeId,
      channel: 'candle',
      symbol,
      interval: interval as any,
      data: candle,
    });
  }

  // Send markets update
  sendMarkets(exchangeId: string, markets: Market[]) {
    this.broadcast({
      type: 'data',
      exchangeId,
      channel: 'market',
      data: markets,
    });
  }

  get clientCount() {
    return this.clients.size;
  }

  async close() {
    return new Promise<void>((resolve) => {
      for (const client of this.clients) {
        client.close();
      }
      this.wss.close(() => resolve());
    });
  }

  get url() {
    return `ws://localhost:${this.port}`;
  }
}

// Minimal mock exchange for testing
class MockExchange extends BaseExchange {
  constructor(options: ExchangeOptions, store: DefaultStore) {
    super('binance', options, store);
  }

  // Override start to NOT call super.start() which throws
  override start = async () => {
    // Initialize external public data if configured
    this.initExternalPublicData();
  };
}

describe('ExternalPublicDataProvider', () => {
  let gateway: MockGatewayServer;
  const TEST_PORT = 18080;

  beforeAll(async () => {
    gateway = new MockGatewayServer(TEST_PORT);
    // Wait for server to be ready
    await new Promise((resolve) => setTimeout(resolve, 100));
  });

  afterAll(async () => {
    await gateway.close();
  });

  describe('Connection and Subscription', () => {
    let store: DefaultStore;
    let exchange: MockExchange;

    beforeEach(async () => {
      store = new DefaultStore();
    });

    afterEach(() => {
      if (exchange) {
        exchange.dispose();
      }
    });

    it('should connect to gateway and receive tickers', async () => {
      const options: ExchangeOptions = {
        key: '',
        secret: '',
        externalPublicData: {
          type: 'websocket',
          url: gateway.url,
          exchangeId: 'binance',
        },
      };

      exchange = new MockExchange(options, store);
      await exchange.start();

      // Wait for connection
      await new Promise((resolve) => setTimeout(resolve, 200));

      expect(gateway.clientCount).toBe(1);

      // Send tickers from gateway
      const mockTickers: Ticker[] = [
        {
          id: 'BTCUSDT',
          symbol: 'BTCUSDT',
          bid: 50000,
          ask: 50001,
          last: 50000.5,
          mark: 50000.5,
          index: 50000,
          percentage: 2.5,
          openInterest: 1000000,
          fundingRate: 0.0001,
          fundingTime: Date.now(),
          volume: 50000,
          quoteVolume: 2500000000,
        },
      ];

      gateway.sendTickers('binance', mockTickers);

      // Wait for message to be processed
      await new Promise((resolve) => setTimeout(resolve, 100));

      // Verify store was updated
      expect(store.tickers.length).toBe(1);
      expect(store.tickers[0].symbol).toBe('BTCUSDT');
      expect(store.tickers[0].last).toBe(50000.5);
      expect(store.loaded.tickers).toBe(true);
    });

    it('should receive orderbook updates via subscription', async () => {
      const options: ExchangeOptions = {
        key: '',
        secret: '',
        externalPublicData: {
          type: 'websocket',
          url: gateway.url,
          exchangeId: 'binance',
        },
      };

      exchange = new MockExchange(options, store);
      await exchange.start();

      // Wait for connection
      await new Promise((resolve) => setTimeout(resolve, 200));

      // Subscribe to orderbook
      const orderbookCallback = vi.fn();
      const unsubscribe = exchange.listenOrderBook('BTCUSDT', orderbookCallback);

      // Wait for subscription to be set up
      await new Promise((resolve) => setTimeout(resolve, 100));

      // Send orderbook from gateway
      const mockOrderbook: OrderBook = {
        bids: [
          { price: 49999, amount: 1.5, total: 1.5 },
          { price: 49998, amount: 2.0, total: 3.5 },
        ],
        asks: [
          { price: 50001, amount: 1.0, total: 1.0 },
          { price: 50002, amount: 1.5, total: 2.5 },
        ],
      };

      gateway.sendOrderbook('binance', 'BTCUSDT', mockOrderbook);

      // Wait for message to be processed
      await new Promise((resolve) => setTimeout(resolve, 100));

      // Verify callback was called with orderbook
      expect(orderbookCallback).toHaveBeenCalledTimes(1);
      expect(orderbookCallback).toHaveBeenCalledWith(mockOrderbook);

      // Cleanup
      unsubscribe();
    });

    it('should receive candle updates via subscription', async () => {
      const options: ExchangeOptions = {
        key: '',
        secret: '',
        externalPublicData: {
          type: 'websocket',
          url: gateway.url,
          exchangeId: 'binance',
        },
      };

      exchange = new MockExchange(options, store);
      await exchange.start();

      // Wait for connection
      await new Promise((resolve) => setTimeout(resolve, 200));

      // Subscribe to candles
      const candleCallback = vi.fn();
      const unsubscribe = exchange.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, candleCallback);

      // Wait for subscription to be set up
      await new Promise((resolve) => setTimeout(resolve, 100));

      // Send candle from gateway
      const mockCandle: Candle = {
        timestamp: Date.now(),
        open: 50000,
        high: 50100,
        low: 49900,
        close: 50050,
        volume: 100,
      };

      gateway.sendCandle('binance', 'BTCUSDT', '1m', mockCandle);

      // Wait for message to be processed
      await new Promise((resolve) => setTimeout(resolve, 100));

      // Verify callback was called with candle
      expect(candleCallback).toHaveBeenCalledTimes(1);
      expect(candleCallback).toHaveBeenCalledWith(mockCandle);

      // Cleanup
      unsubscribe();
    });

    it('should receive markets and update store', async () => {
      const options: ExchangeOptions = {
        key: '',
        secret: '',
        externalPublicData: {
          type: 'websocket',
          url: gateway.url,
          exchangeId: 'binance',
        },
      };

      exchange = new MockExchange(options, store);
      await exchange.start();

      // Wait for connection
      await new Promise((resolve) => setTimeout(resolve, 200));

      // Send markets from gateway
      const mockMarkets: Market[] = [
        {
          id: 'BTCUSDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 3, price: 2 },
          limits: {
            amount: { min: 0.001, max: 1000 },
            leverage: { min: 1, max: 125 },
          },
        },
        {
          id: 'ETHUSDT',
          symbol: 'ETHUSDT',
          base: 'ETH',
          quote: 'USDT',
          active: true,
          precision: { amount: 3, price: 2 },
          limits: {
            amount: { min: 0.01, max: 10000 },
            leverage: { min: 1, max: 100 },
          },
        },
      ];

      gateway.sendMarkets('binance', mockMarkets);

      // Wait for message to be processed
      await new Promise((resolve) => setTimeout(resolve, 100));

      // Verify store was updated
      expect(store.markets.length).toBe(2);
      expect(store.markets[0].symbol).toBe('BTCUSDT');
      expect(store.markets[1].symbol).toBe('ETHUSDT');
      expect(store.loaded.markets).toBe(true);
    });

    it('should ignore messages from other exchanges', async () => {
      const options: ExchangeOptions = {
        key: '',
        secret: '',
        externalPublicData: {
          type: 'websocket',
          url: gateway.url,
          exchangeId: 'binance',
        },
      };

      exchange = new MockExchange(options, store);
      await exchange.start();

      // Wait for connection
      await new Promise((resolve) => setTimeout(resolve, 200));

      // Send tickers for a different exchange
      gateway.sendTickers('bybit', [
        {
          id: 'BTCUSDT',
          symbol: 'BTCUSDT',
          bid: 60000,
          ask: 60001,
          last: 60000.5,
          mark: 60000.5,
          index: 60000,
          percentage: 3.0,
          openInterest: 2000000,
          fundingRate: 0.0002,
          fundingTime: Date.now(),
          volume: 60000,
          quoteVolume: 3600000000,
        },
      ]);

      // Wait for message to be processed
      await new Promise((resolve) => setTimeout(resolve, 100));

      // Verify store was NOT updated (different exchangeId)
      expect(store.tickers.length).toBe(0);
    });

    it('should unsubscribe from orderbook correctly', async () => {
      const options: ExchangeOptions = {
        key: '',
        secret: '',
        externalPublicData: {
          type: 'websocket',
          url: gateway.url,
          exchangeId: 'binance',
        },
      };

      exchange = new MockExchange(options, store);
      await exchange.start();

      // Wait for connection
      await new Promise((resolve) => setTimeout(resolve, 200));

      // Subscribe to orderbook
      const orderbookCallback = vi.fn();
      const unsubscribe = exchange.listenOrderBook('BTCUSDT', orderbookCallback);

      // Wait for subscription
      await new Promise((resolve) => setTimeout(resolve, 100));

      // Send first orderbook
      gateway.sendOrderbook('binance', 'BTCUSDT', {
        bids: [{ price: 50000, amount: 1, total: 1 }],
        asks: [{ price: 50001, amount: 1, total: 1 }],
      });

      await new Promise((resolve) => setTimeout(resolve, 100));
      expect(orderbookCallback).toHaveBeenCalledTimes(1);

      // Unsubscribe
      unsubscribe();

      // Send second orderbook - should NOT trigger callback
      gateway.sendOrderbook('binance', 'BTCUSDT', {
        bids: [{ price: 50002, amount: 2, total: 2 }],
        asks: [{ price: 50003, amount: 2, total: 2 }],
      });

      await new Promise((resolve) => setTimeout(resolve, 100));
      // Should still be 1, not 2
      expect(orderbookCallback).toHaveBeenCalledTimes(1);
    });
  });

  describe('Backward Compatibility', () => {
    it('should work without externalPublicData (direct connection mode)', async () => {
      const store = new DefaultStore();

      // No externalPublicData - should use direct connection
      const options: ExchangeOptions = {
        key: '',
        secret: '',
      };

      const exchange = new MockExchange(options, store);

      // Get initial client count before start
      const initialClientCount = gateway.clientCount;

      await exchange.start();

      // usesExternalPublicData should be false
      expect(exchange.usesExternalPublicData).toBe(false);
      expect(exchange.externalPublicDataProvider).toBeUndefined();

      // No NEW connection to gateway (count should be same as before)
      expect(gateway.clientCount).toBe(initialClientCount);

      exchange.dispose();
    });
  });
});

describe('ExternalPublicDataProvider - Edge Cases', () => {
  let gateway: MockGatewayServer;
  const TEST_PORT = 18081;

  beforeEach(async () => {
    gateway = new MockGatewayServer(TEST_PORT);
    await new Promise((resolve) => setTimeout(resolve, 100));
  });

  afterEach(async () => {
    await gateway.close();
  });

  it('should handle multiple subscribers to same orderbook', async () => {
    const store = new DefaultStore();
    const options: ExchangeOptions = {
      key: '',
      secret: '',
      externalPublicData: {
        type: 'websocket',
        url: gateway.url,
        exchangeId: 'binance',
      },
    };

    const exchange = new MockExchange(options, store);
    await exchange.start();
    await new Promise((resolve) => setTimeout(resolve, 200));

    // Two subscribers to same symbol
    const callback1 = vi.fn();
    const callback2 = vi.fn();

    const unsub1 = exchange.listenOrderBook('BTCUSDT', callback1);
    const unsub2 = exchange.listenOrderBook('BTCUSDT', callback2);

    await new Promise((resolve) => setTimeout(resolve, 100));

    // Send orderbook
    gateway.sendOrderbook('binance', 'BTCUSDT', {
      bids: [{ price: 50000, amount: 1, total: 1 }],
      asks: [{ price: 50001, amount: 1, total: 1 }],
    });

    await new Promise((resolve) => setTimeout(resolve, 100));

    // Both callbacks should be called
    expect(callback1).toHaveBeenCalledTimes(1);
    expect(callback2).toHaveBeenCalledTimes(1);

    // Unsubscribe one
    unsub1();

    // Send another orderbook
    gateway.sendOrderbook('binance', 'BTCUSDT', {
      bids: [{ price: 50002, amount: 2, total: 2 }],
      asks: [{ price: 50003, amount: 2, total: 2 }],
    });

    await new Promise((resolve) => setTimeout(resolve, 100));

    // Only callback2 should receive the second message
    expect(callback1).toHaveBeenCalledTimes(1);
    expect(callback2).toHaveBeenCalledTimes(2);

    unsub2();
    exchange.dispose();
  });
});
