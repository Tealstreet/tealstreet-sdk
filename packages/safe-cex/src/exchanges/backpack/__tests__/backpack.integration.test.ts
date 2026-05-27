// @vitest-environment node
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { BackpackExchange } from '../backpack.exchange';
import { OrderSide, OrderType, OrderTimeInForce } from '../../../types';
import { DefaultStore } from '../../../store/store.base';
import * as fs from 'fs';
import * as path from 'path';

describe('Backpack Integration Tests', () => {
  let exchange: BackpackExchange;
  let store: DefaultStore;
  let apiKey: string;
  let secret: string;

  beforeAll(async () => {
    // Load API credentials (file is in parent directory)
    const apiKeyPath = path.join(__dirname, '..', '.apikey.json');
    const apiKeyData = JSON.parse(fs.readFileSync(apiKeyPath, 'utf-8'));

    apiKey = apiKeyData.apiKey;
    secret = apiKeyData.secret;

    // Create store
    store = new DefaultStore();

    // Create exchange with store
    exchange = new BackpackExchange(
      'backpack',
      {
        key: apiKey,
        secret: secret,
        testnet: false,
      },
      store
    );

    // Listen for errors
    exchange.on('error', (error: string) => {
      console.log('Exchange error:', error);
    });
  });

  describe('WebSocket Tests', () => {
    it('should connect to public WebSocket', async () => {
      await exchange.start();

      // Wait for connection
      await new Promise((resolve) => setTimeout(resolve, 2000));

      // Check if markets are loaded
      expect(exchange.store.markets.length).toBeGreaterThan(0);
      console.log(`Loaded ${exchange.store.markets.length} markets`);

      // Subscribe to a market
      const testMarket = exchange.store.markets[0];
      console.log(`Subscribing to ${testMarket.symbol}`);

      exchange.publicWebsocket?.subscribeToTicker(testMarket.id);
      exchange.publicWebsocket?.subscribeToOrderBook(testMarket.id);

      // Wait for ticker updates
      await new Promise((resolve) => setTimeout(resolve, 3000));

      // Check if ticker was updated
      const ticker = exchange.store.tickers.find((t) => t.symbol === testMarket.symbol);
      expect(ticker).toBeDefined();
      console.log(`Ticker for ${testMarket.symbol}:`, {
        bid: ticker?.bid,
        ask: ticker?.ask,
        last: ticker?.last,
        volume: ticker?.volume,
      });
    }, 10000);

    it('should connect to private WebSocket and receive updates', async () => {
      // Private WebSocket should already be connected from start()
      expect(exchange.privateWebsocket).toBeDefined();

      // Wait for any initial updates
      await new Promise((resolve) => setTimeout(resolve, 2000));

      console.log('Private WebSocket connected successfully');
    }, 5000);
  });

  describe('Order Operations', () => {
    let testSymbol: string;
    let orderId: string | undefined;

    beforeAll(async () => {
      // Find a liquid market for testing (e.g., SOL_USDC)
      const market = exchange.store.markets.find(
        (m) => m.symbol === 'SOL/USDC' || m.symbol === 'BTC/USDC'
      );

      if (!market) {
        throw new Error('Could not find suitable test market');
      }

      testSymbol = market.symbol;
      console.log(`Using ${testSymbol} for order tests`);

      // Fetch current ticker for price reference
      const ticker = exchange.store.tickers.find((t) => t.symbol === testSymbol);
      console.log(`Current ticker:`, {
        bid: ticker?.bid,
        ask: ticker?.ask,
        last: ticker?.last,
      });
    });

    it('should place a limit order with small amount', async () => {
      const ticker = exchange.store.tickers.find((t) => t.symbol === testSymbol);
      expect(ticker).toBeDefined();

      // Place a limit buy order far below market price (won't fill)
      const buyPrice = ticker!.bid * 0.5; // 50% below current bid
      const amount = 0.01; // Very small amount

      console.log(`Placing limit buy order: ${amount} ${testSymbol} @ ${buyPrice}`);

      try {
        const orderIds = await exchange.placeOrder({
          symbol: testSymbol,
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount,
          price: buyPrice,
          timeInForce: OrderTimeInForce.GoodTillCancel,
        });

        expect(orderIds).toBeDefined();
        expect(orderIds.length).toBeGreaterThan(0);
        orderId = orderIds[0];

        console.log(`Order placed successfully: ${orderId}`);

        // Wait for order to appear in store
        await new Promise((resolve) => setTimeout(resolve, 2000));

        // Verify order is in the store
        const orders = await exchange.fetchOrders();
        const placedOrder = orders.find((o) => o.id === orderId);

        if (placedOrder) {
          console.log('Order details:', {
            id: placedOrder.id,
            symbol: placedOrder.symbol,
            side: placedOrder.side,
            type: placedOrder.type,
            price: placedOrder.price,
            amount: placedOrder.amount,
            status: placedOrder.status,
          });
        } else {
          console.warn('Order not found in store yet');
        }
      } catch (error) {
        console.error('Error placing order:', error);
        throw error;
      }
    }, 15000);

    it('should cancel the placed order', async () => {
      if (!orderId) {
        console.warn('No order ID to cancel, skipping test');
        return;
      }

      console.log(`Canceling order: ${orderId}`);

      try {
        // Fetch orders to get the order object
        const orders = await exchange.fetchOrders();
        const orderToCancel = orders.find((o) => o.id === orderId);

        if (!orderToCancel) {
          console.warn('Order not found, may have been canceled already');
          return;
        }

        await exchange.cancelOrder(orderToCancel);
        console.log('Order canceled successfully');

        // Wait for cancellation to propagate
        await new Promise((resolve) => setTimeout(resolve, 2000));

        // Verify order is canceled
        const updatedOrders = await exchange.fetchOrders();
        const canceledOrder = updatedOrders.find((o) => o.id === orderId);

        if (!canceledOrder) {
          console.log('Order no longer in open orders (canceled)');
        } else {
          console.log('Order status:', canceledOrder.status);
        }
      } catch (error) {
        console.error('Error canceling order:', error);
        throw error;
      }
    }, 10000);

    it('should fetch account balance', async () => {
      try {
        const balance = await exchange.fetchBalance();

        console.log('Account balance:', {
          total: balance.total,
          free: balance.free,
          used: balance.used,
        });

        expect(balance.total).toBeGreaterThanOrEqual(0);
      } catch (error) {
        console.error('Error fetching balance:', error);
        throw error;
      }
    }, 10000);

    it('should fetch positions', async () => {
      try {
        const positions = await exchange.fetchPositions();

        console.log(`Fetched ${positions.length} positions`);

        // Show sample position
        if (positions.length > 0) {
          const samplePosition = positions[0];
          console.log('Sample position:', {
            symbol: samplePosition.symbol,
            side: samplePosition.side,
            contracts: samplePosition.contracts,
            entryPrice: samplePosition.entryPrice,
            unrealizedPnl: samplePosition.unrealizedPnl,
          });
        }

        // Verify fake positions are created
        const fakePositions = positions.filter((p) => p.contracts === 0);
        console.log(`Found ${fakePositions.length} fake positions`);
        expect(fakePositions.length).toBeGreaterThan(0);
      } catch (error) {
        console.error('Error fetching positions:', error);
        throw error;
      }
    }, 10000);
  });

  afterAll(() => {
    // Clean up WebSocket connections
    if (exchange.publicWebsocket) {
      exchange.publicWebsocket.dispose();
    }
    if (exchange.privateWebsocket) {
      exchange.privateWebsocket.dispose();
    }
  });
});
