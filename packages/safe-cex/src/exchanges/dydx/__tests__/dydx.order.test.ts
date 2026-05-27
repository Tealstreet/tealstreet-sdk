import { describe, it, expect, beforeEach, vi } from 'vitest';
import { DydxExchange } from '../dydx.exchange';
import { DefaultStore } from '../../../store/store.base';
import { OrderSide, OrderType, OrderTimeInForce, OrderStatus, ExchangeOptions } from '../../../types';
import { DydxOrderType, DydxOrderSide, DydxTimeInForce, DydxOrderStatus } from '../dydx.types';

/**
 * Unit tests for dYdX order management (placeOrder, fetchOrders)
 * Uses mocks to avoid real API calls
 */
describe('DydxExchange - Order Management (with mocks)', () => {
  let exchange: DydxExchange;
  let store: DefaultStore;
  let mockSdkClient: any;

  beforeEach(() => {
    // Create exchange with test options
    const options: ExchangeOptions = {
      key: 'dydx1test123',
      secret: 'test mnemonic phrase',
      testnet: true,
    };

    store = new DefaultStore();
    exchange = new DydxExchange('dydx', options, store);

    // Mock SDK client
    mockSdkClient = {
      getCurrentBlockHeight: vi.fn().mockResolvedValue(1000),
      placeShortTermOrder: vi.fn().mockResolvedValue({
        hash: '0xabc123',
        code: 0,
      }),
      placeOrder: vi.fn().mockResolvedValue({
        hash: '0xdef456',
        code: 0,
      }),
      getOrders: vi.fn().mockResolvedValue([]),
      getSubaccount: vi.fn().mockResolvedValue({
        subaccount: {
          equity: '1000.0',
        },
      }),
    };

    // Inject mocked SDK client
    exchange.sdkClient = mockSdkClient;

    // Set up mock markets in store
    store.update({
      markets: [
        {
          id: 'ETH-USD',
          symbol: 'ETH-USD',
          base: 'ETH',
          quote: 'USD',
          active: true,
          precision: {
            amount: 0.001,
            price: 0.1,
          },
          limits: {
            amount: {
              min: 0.001,
              max: 1000,
            },
            leverage: {
              min: 1,
              max: 20,
            },
          },
          contractSize: 1,
          takerFee: 0.0005,
          makerFee: 0.00002,
          info: {},
        },
        {
          id: 'BTC-USD',
          symbol: 'BTC-USD',
          base: 'BTC',
          quote: 'USD',
          active: true,
          precision: {
            amount: 0.0001,
            price: 1,
          },
          limits: {
            amount: {
              min: 0.0001,
              max: 100,
            },
            leverage: {
              min: 1,
              max: 20,
            },
          },
          contractSize: 1,
          takerFee: 0.0005,
          makerFee: 0.00002,
          info: {},
        },
      ],
    });
  });

  describe('placeOrder - Limit Orders', () => {
    it('should place a limit BUY order successfully', async () => {
      const orderIds = await exchange.placeOrder({
        symbol: 'ETH-USD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 2500,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);
      expect(typeof orderIds[0]).toBe('string');

      // Verify SDK method was called
      expect(mockSdkClient.placeShortTermOrder).toHaveBeenCalledTimes(1);

      const callArgs = mockSdkClient.placeShortTermOrder.mock.calls[0][0];
      expect(callArgs.market).toBe('ETH-USD');
      expect(callArgs.side).toBe(DydxOrderSide.BUY);
      expect(callArgs.price).toBe(2500);
      expect(callArgs.size).toBe(0.1);
      expect(callArgs.reduceOnly).toBe(false);
      expect(callArgs.timeInForce).toBe(DydxTimeInForce.GTT);
      expect(callArgs.clientId).toBeGreaterThan(0);
      expect(callArgs.goodTilBlock).toBeGreaterThan(1000);
    });

    it('should place a limit SELL order successfully', async () => {
      const orderIds = await exchange.placeOrder({
        symbol: 'BTC-USD',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        amount: 0.01,
        price: 50000,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);

      // Verify SDK method was called with correct parameters
      expect(mockSdkClient.placeShortTermOrder).toHaveBeenCalledTimes(1);

      const callArgs = mockSdkClient.placeShortTermOrder.mock.calls[0][0];
      expect(callArgs.market).toBe('BTC-USD');
      expect(callArgs.side).toBe(DydxOrderSide.SELL);
      expect(callArgs.price).toBe(50000);
      expect(callArgs.size).toBe(0.01);
    });

    it('should place a POST_ONLY limit order', async () => {
      const orderIds = await exchange.placeOrder({
        symbol: 'ETH-USD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.5,
        price: 2400,
        timeInForce: OrderTimeInForce.PostOnly,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);

      const callArgs = mockSdkClient.placeShortTermOrder.mock.calls[0][0];
      expect(callArgs.timeInForce).toBe(DydxTimeInForce.GTT);
      // PostOnly is handled via execution flag in the SDK
    });

    it('should place a reduce-only limit order', async () => {
      const orderIds = await exchange.placeOrder({
        symbol: 'ETH-USD',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        amount: 0.2,
        price: 2600,
        reduceOnly: true,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);

      const callArgs = mockSdkClient.placeShortTermOrder.mock.calls[0][0];
      expect(callArgs.reduceOnly).toBe(true);
    });

    it('should generate unique client IDs for multiple orders', async () => {
      await exchange.placeOrder({
        symbol: 'ETH-USD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 2500,
      });

      await exchange.placeOrder({
        symbol: 'ETH-USD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 2501,
      });

      await exchange.placeOrder({
        symbol: 'ETH-USD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 2502,
      });

      expect(mockSdkClient.placeShortTermOrder).toHaveBeenCalledTimes(3);

      const clientId1 = mockSdkClient.placeShortTermOrder.mock.calls[0][0].clientId;
      const clientId2 = mockSdkClient.placeShortTermOrder.mock.calls[1][0].clientId;
      const clientId3 = mockSdkClient.placeShortTermOrder.mock.calls[2][0].clientId;

      // All client IDs should be unique
      expect(clientId1).not.toBe(clientId2);
      expect(clientId2).not.toBe(clientId3);
      expect(clientId1).not.toBe(clientId3);
    });

    it('should throw error if market not found', async () => {
      await expect(
        exchange.placeOrder({
          symbol: 'INVALID-USD',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 1,
          price: 100,
        })
      ).rejects.toThrow('Market INVALID-USD not found');
    });

    it('should throw error if SDK client not initialized', async () => {
      exchange.sdkClient = null;

      await expect(
        exchange.placeOrder({
          symbol: 'ETH-USD',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 0.1,
          price: 2500,
        })
      ).rejects.toThrow('SDK client not initialized');
    });

    it('should handle SDK errors gracefully', async () => {
      mockSdkClient.placeShortTermOrder.mockRejectedValueOnce(
        new Error('Insufficient margin')
      );

      await expect(
        exchange.placeOrder({
          symbol: 'ETH-USD',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 0.1,
          price: 2500,
        })
      ).rejects.toThrow('Insufficient margin');
    });
  });

  describe('fetchOrders', () => {
    it('should fetch orders for a specific symbol', async () => {
      // Mock API response with orders
      mockSdkClient.getOrders.mockResolvedValueOnce([
        {
          id: 'order1',
          clientId: '123456',
          clobPairId: 'ETH-USD',
          side: DydxOrderSide.BUY,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.OPEN,
          price: '2500',
          size: '0.1',
          totalFilled: '0',
          reduceOnly: false,
          createdAtHeight: '1000',
        },
        {
          id: 'order2',
          clientId: '123457',
          clobPairId: 'ETH-USD',
          side: DydxOrderSide.SELL,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.PARTIALLY_FILLED,
          price: '2600',
          size: '0.2',
          totalFilled: '0.05',
          reduceOnly: false,
          createdAtHeight: '1001',
        },
      ]);

      const orders = await exchange.fetchOrders(['ETH-USD']);

      expect(orders).toBeDefined();
      expect(orders.length).toBe(2);

      // Verify first order
      expect(orders[0].id).toBe('order1');
      expect(orders[0].symbol).toBe('ETH-USD');
      expect(orders[0].side).toBe(OrderSide.Buy);
      expect(orders[0].type).toBe(OrderType.Limit);
      expect(orders[0].price).toBe(2500);
      expect(orders[0].amount).toBe(0.1);
      expect(orders[0].filled).toBe(0);
      expect(orders[0].remaining).toBe(0.1);

      // Verify second order
      expect(orders[1].id).toBe('order2');
      expect(orders[1].symbol).toBe('ETH-USD');
      expect(orders[1].side).toBe(OrderSide.Sell);
      expect(orders[1].type).toBe(OrderType.Limit);
      expect(orders[1].price).toBe(2600);
      expect(orders[1].amount).toBe(0.2);
      expect(orders[1].filled).toBe(0.05);
      expect(orders[1].remaining).toBeCloseTo(0.15, 10); // Use toBeCloseTo for floating point

      // Verify SDK was called with correct parameters
      expect(mockSdkClient.getOrders).toHaveBeenCalledWith(
        'dydx1test123',
        0,
        'ETH-USD'
      );
    });

    it('should fetch all orders when no symbol specified', async () => {
      mockSdkClient.getOrders.mockResolvedValueOnce([
        {
          id: 'order1',
          clientId: '123456',
          clobPairId: 'ETH-USD',
          side: DydxOrderSide.BUY,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.OPEN,
          price: '2500',
          size: '0.1',
          totalFilled: '0',
          reduceOnly: false,
          createdAtHeight: '1000',
        },
        {
          id: 'order2',
          clientId: '123457',
          clobPairId: 'BTC-USD',
          side: DydxOrderSide.SELL,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.OPEN,
          price: '50000',
          size: '0.01',
          totalFilled: '0',
          reduceOnly: false,
          createdAtHeight: '1001',
        },
      ]);

      const orders = await exchange.fetchOrders();

      expect(orders).toBeDefined();
      expect(orders.length).toBe(2);
      expect(orders[0].symbol).toBe('ETH-USD');
      expect(orders[1].symbol).toBe('BTC-USD');

      // Verify SDK was called without market filter
      expect(mockSdkClient.getOrders).toHaveBeenCalledWith(
        'dydx1test123',
        0,
        undefined
      );
    });

    it('should return empty array when no orders found', async () => {
      mockSdkClient.getOrders.mockResolvedValueOnce([]);

      const orders = await exchange.fetchOrders(['ETH-USD']);

      expect(orders).toBeDefined();
      expect(orders.length).toBe(0);
    });

    it('should filter out non-open orders', async () => {
      mockSdkClient.getOrders.mockResolvedValueOnce([
        {
          id: 'order1',
          clientId: '123456',
          clobPairId: 'ETH-USD',
          side: DydxOrderSide.BUY,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.OPEN,
          price: '2500',
          size: '0.1',
          totalFilled: '0',
          reduceOnly: false,
          createdAtHeight: '1000',
        },
        {
          id: 'order2',
          clientId: '123457',
          clobPairId: 'ETH-USD',
          side: DydxOrderSide.SELL,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.FILLED,
          price: '2600',
          size: '0.2',
          totalFilled: '0.2',
          reduceOnly: false,
          createdAtHeight: '1001',
        },
        {
          id: 'order3',
          clientId: '123458',
          clobPairId: 'ETH-USD',
          side: DydxOrderSide.BUY,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.CANCELED,
          price: '2400',
          size: '0.15',
          totalFilled: '0',
          reduceOnly: false,
          createdAtHeight: '1002',
        },
        {
          id: 'order4',
          clientId: '123459',
          clobPairId: 'ETH-USD',
          side: DydxOrderSide.BUY,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.PARTIALLY_FILLED,
          price: '2450',
          size: '0.3',
          totalFilled: '0.1',
          reduceOnly: false,
          createdAtHeight: '1003',
        },
      ]);

      const orders = await exchange.fetchOrders(['ETH-USD']);

      // Should only include OPEN and PARTIALLY_FILLED orders
      expect(orders.length).toBe(2);
      expect(orders[0].id).toBe('order1');
      expect(orders[1].id).toBe('order4');
    });

    it('should handle 404 error for empty accounts', async () => {
      const notFoundError: any = new Error('Not found');
      notFoundError.response = { status: 404 };
      mockSdkClient.getOrders.mockRejectedValueOnce(notFoundError);

      const orders = await exchange.fetchOrders(['ETH-USD']);

      // Should return empty array for 404
      expect(orders).toBeDefined();
      expect(orders.length).toBe(0);
    });

    it('should throw error if SDK client not initialized', async () => {
      exchange.sdkClient = null;

      await expect(exchange.fetchOrders(['ETH-USD'])).rejects.toThrow(
        'SDK client not initialized'
      );
    });

    it('should preserve clientId in order info', async () => {
      mockSdkClient.getOrders.mockResolvedValueOnce([
        {
          id: 'order1',
          clientId: '987654',
          clobPairId: 'ETH-USD',
          side: DydxOrderSide.BUY,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.OPEN,
          price: '2500',
          size: '0.1',
          totalFilled: '0',
          reduceOnly: false,
          createdAtHeight: '1000',
        },
      ]);

      const orders = await exchange.fetchOrders(['ETH-USD']);

      expect(orders[0].info).toBeDefined();
      expect((orders[0].info as any).clientId).toBe('987654');
    });
  });

  describe('Order Flow Integration', () => {
    it('should successfully place order and fetch it back', async () => {
      // Step 1: Place order
      const orderIds = await exchange.placeOrder({
        symbol: 'ETH-USD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 2500,
      });

      const clientId = orderIds[0];

      // Step 2: Mock fetchOrders to return the order we just placed
      mockSdkClient.getOrders.mockResolvedValueOnce([
        {
          id: 'order_hash',
          clientId: clientId,
          clobPairId: 'ETH-USD',
          side: DydxOrderSide.BUY,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.OPEN,
          price: '2500',
          size: '0.1',
          totalFilled: '0',
          reduceOnly: false,
          createdAtHeight: '1000',
        },
      ]);

      // Step 3: Fetch orders
      const orders = await exchange.fetchOrders(['ETH-USD']);

      // Step 4: Verify the order is found
      expect(orders.length).toBe(1);
      expect((orders[0].info as any).clientId).toBe(clientId);
      expect(orders[0].symbol).toBe('ETH-USD');
      expect(orders[0].side).toBe(OrderSide.Buy);
      expect(orders[0].price).toBe(2500);
      expect(orders[0].amount).toBe(0.1);
    });
  });

  describe('cancelOrder', () => {
    it('should cancel an order successfully', async () => {
      // Mock cancelOrder SDK method
      mockSdkClient.cancelOrder = vi.fn().mockResolvedValue({
        hash: '0xcancel123',
        code: 0,
      });

      const order = {
        id: 'order1',
        symbol: 'ETH-USD',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        price: 2500,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: false,
        lastUpdate: Date.now(),
        info: {
          clientId: '123456',
        },
      };

      await exchange.cancelOrder(order);

      expect(mockSdkClient.cancelOrder).toHaveBeenCalledTimes(1);

      const callArgs = mockSdkClient.cancelOrder.mock.calls[0][0];
      expect(callArgs.clientId).toBe(123456);
      expect(callArgs.orderFlags).toBe(0); // DydxOrderFlags.SHORT_TERM
      expect(callArgs.clobPairId).toBe('ETH-USD');
      expect(callArgs.goodTilBlock).toBeGreaterThan(1000);
    });

    it('should throw error if SDK client not initialized', async () => {
      exchange.sdkClient = null;

      const order = {
        id: 'order1',
        symbol: 'ETH-USD',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        price: 2500,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: false,
        lastUpdate: Date.now(),
        info: { clientId: '123456' },
      };

      await expect(exchange.cancelOrder(order)).rejects.toThrow(
        'SDK client not initialized'
      );
    });

    it('should throw error if market not found', async () => {
      mockSdkClient.cancelOrder = vi.fn().mockResolvedValue({
        hash: '0xcancel123',
        code: 0,
      });

      const order = {
        id: 'order1',
        symbol: 'INVALID-USD',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        price: 2500,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: false,
        lastUpdate: Date.now(),
        info: { clientId: '123456' },
      };

      await expect(exchange.cancelOrder(order)).rejects.toThrow(
        'Market INVALID-USD not found'
      );
    });

    it('should use order.id as clientId if clientId not in info', async () => {
      mockSdkClient.cancelOrder = vi.fn().mockResolvedValue({
        hash: '0xcancel123',
        code: 0,
      });

      const order = {
        id: '789456',
        symbol: 'ETH-USD',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        price: 2500,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: false,
        lastUpdate: Date.now(),
        info: {},
      };

      await exchange.cancelOrder(order);

      const callArgs = mockSdkClient.cancelOrder.mock.calls[0][0];
      expect(callArgs.clientId).toBe(789456);
    });

    it('should handle SDK errors gracefully', async () => {
      mockSdkClient.cancelOrder = vi.fn().mockRejectedValue(
        new Error('Order not found on chain')
      );

      const order = {
        id: 'order1',
        symbol: 'ETH-USD',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        price: 2500,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: false,
        lastUpdate: Date.now(),
        info: { clientId: '123456' },
      };

      await expect(exchange.cancelOrder(order)).rejects.toThrow(
        'Order not found on chain'
      );
    });
  });

  describe('cancelOrders', () => {
    it('should cancel multiple orders successfully', async () => {
      mockSdkClient.cancelOrder = vi.fn().mockResolvedValue({
        hash: '0xcancel123',
        code: 0,
      });

      const orders = [
        {
          id: 'order1',
          symbol: 'ETH-USD',
          side: OrderSide.Buy,
          type: OrderType.Limit,
          status: OrderStatus.Open,
          price: 2500,
          amount: 0.1,
          filled: 0,
          remaining: 0.1,
          reduceOnly: false,
          lastUpdate: Date.now(),
          info: { clientId: '123456' },
        },
        {
          id: 'order2',
          symbol: 'BTC-USD',
          side: OrderSide.Sell,
          type: OrderType.Limit,
          status: OrderStatus.Open,
          price: 50000,
          amount: 0.01,
          filled: 0,
          remaining: 0.01,
          reduceOnly: false,
          lastUpdate: Date.now(),
          info: { clientId: '123457' },
        },
      ];

      await exchange.cancelOrders(orders);

      expect(mockSdkClient.cancelOrder).toHaveBeenCalledTimes(2);
    });

    it('should continue cancelling even if one order fails', async () => {
      let callCount = 0;
      mockSdkClient.cancelOrder = vi.fn().mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.reject(new Error('First order failed'));
        }
        return Promise.resolve({ hash: '0xcancel123', code: 0 });
      });

      const orders = [
        {
          id: 'order1',
          symbol: 'ETH-USD',
          side: OrderSide.Buy,
          type: OrderType.Limit,
          status: OrderStatus.Open,
          price: 2500,
          amount: 0.1,
          filled: 0,
          remaining: 0.1,
          reduceOnly: false,
          lastUpdate: Date.now(),
          info: { clientId: '123456' },
        },
        {
          id: 'order2',
          symbol: 'BTC-USD',
          side: OrderSide.Sell,
          type: OrderType.Limit,
          status: OrderStatus.Open,
          price: 50000,
          amount: 0.01,
          filled: 0,
          remaining: 0.01,
          reduceOnly: false,
          lastUpdate: Date.now(),
          info: { clientId: '123457' },
        },
      ];

      // Should not throw even though first order failed
      await exchange.cancelOrders(orders);

      // Both orders should have been attempted
      expect(mockSdkClient.cancelOrder).toHaveBeenCalledTimes(2);
    });
  });

  describe('cancelAllOrders', () => {
    it('should fetch and cancel all open orders', async () => {
      mockSdkClient.getOrders = vi.fn().mockResolvedValue([
        {
          id: 'order1',
          clientId: '123456',
          clobPairId: 'ETH-USD',
          side: DydxOrderSide.BUY,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.OPEN,
          price: '2500',
          size: '0.1',
          totalFilled: '0',
          reduceOnly: false,
          createdAtHeight: '1000',
        },
        {
          id: 'order2',
          clientId: '123457',
          clobPairId: 'BTC-USD',
          side: DydxOrderSide.SELL,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.OPEN,
          price: '50000',
          size: '0.01',
          totalFilled: '0',
          reduceOnly: false,
          createdAtHeight: '1001',
        },
      ]);

      mockSdkClient.cancelOrder = vi.fn().mockResolvedValue({
        hash: '0xcancel123',
        code: 0,
      });

      await exchange.cancelAllOrders();

      // Should fetch orders once
      expect(mockSdkClient.getOrders).toHaveBeenCalledTimes(1);

      // Should cancel both orders
      expect(mockSdkClient.cancelOrder).toHaveBeenCalledTimes(2);
    });
  });

  describe('cancelSymbolOrders', () => {
    it('should fetch and cancel orders for specific symbol', async () => {
      mockSdkClient.getOrders = vi.fn().mockResolvedValue([
        {
          id: 'order1',
          clientId: '123456',
          clobPairId: 'ETH-USD',
          side: DydxOrderSide.BUY,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.OPEN,
          price: '2500',
          size: '0.1',
          totalFilled: '0',
          reduceOnly: false,
          createdAtHeight: '1000',
        },
      ]);

      mockSdkClient.cancelOrder = vi.fn().mockResolvedValue({
        hash: '0xcancel123',
        code: 0,
      });

      await exchange.cancelSymbolOrders('ETH-USD');

      // Should fetch orders for ETH-USD
      expect(mockSdkClient.getOrders).toHaveBeenCalledWith(
        'dydx1test123',
        0,
        'ETH-USD'
      );

      // Should cancel the order
      expect(mockSdkClient.cancelOrder).toHaveBeenCalledTimes(1);
    });
  });

  describe('Complete Order Lifecycle', () => {
    it('should place, fetch, and cancel an order successfully', async () => {
      // Step 1: Place order
      const orderIds = await exchange.placeOrder({
        symbol: 'ETH-USD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 2500,
      });

      const clientId = orderIds[0];

      // Step 2: Mock fetchOrders to return the order
      mockSdkClient.getOrders.mockResolvedValueOnce([
        {
          id: 'order_hash',
          clientId: clientId,
          clobPairId: 'ETH-USD',
          side: DydxOrderSide.BUY,
          type: DydxOrderType.LIMIT,
          status: DydxOrderStatus.OPEN,
          price: '2500',
          size: '0.1',
          totalFilled: '0',
          reduceOnly: false,
          createdAtHeight: '1000',
        },
      ]);

      const orders = await exchange.fetchOrders(['ETH-USD']);
      expect(orders.length).toBe(1);

      // Step 3: Cancel the order
      mockSdkClient.cancelOrder = vi.fn().mockResolvedValue({
        hash: '0xcancel123',
        code: 0,
      });

      await exchange.cancelOrder(orders[0]);

      expect(mockSdkClient.cancelOrder).toHaveBeenCalledTimes(1);

      const cancelArgs = mockSdkClient.cancelOrder.mock.calls[0][0];
      expect(cancelArgs.clientId).toBe(parseInt(clientId));
    });
  });
});
