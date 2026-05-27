import { describe, it, expect, beforeEach, vi } from 'vitest';
import { GrvtExchange } from '../grvt.exchange';
import { DefaultStore } from '../../../store/store.base';
import { OrderSide, OrderType, OrderTimeInForce, OrderStatus, ExchangeOptions } from '../../../types';

/**
 * Unit tests for GRVT order management (placeOrder, fetchOrders, cancelOrder)
 * Uses mocks to avoid real API calls
 */
describe('GrvtExchange - Order Management (with mocks)', () => {
  let exchange: GrvtExchange;
  let store: DefaultStore;
  let mockTradingXhr: any;

  beforeEach(() => {
    // Create exchange with test options
    const options: ExchangeOptions = {
      key: 'test_api_key',
      secret: '0x' + '0'.repeat(64), // Mock private key
      testnet: true,
    };

    store = new DefaultStore();
    exchange = new GrvtExchange('grvt', options, store);

    // Mock the trading XHR client
    mockTradingXhr = {
      post: vi.fn(),
    };

    // Replace the exchange's tradingXhr with our mock
    exchange.tradingXhr = mockTradingXhr as any;

    // Mock authenticated session
    exchange.apiExtras = {
      accountId: 'test_account_123',
      subAccountId: 'test_subaccount_456',
      sessionCookie: 'mock_session_cookie',
    };

    // Set up mock markets in store
    store.update({
      markets: [
        {
          id: 'BTC_USDT_Perp',
          symbol: 'BTC_USDT_Perp',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: {
            amount: 0.001,
            price: 0.1,
          },
          limits: {
            amount: {
              min: 0.001,
              max: 100,
            },
            leverage: {
              min: 1,
              max: 100,
            },
          },
          contractSize: 1,
        },
        {
          id: 'ETH_USDT_Perp',
          symbol: 'ETH_USDT_Perp',
          base: 'ETH',
          quote: 'USDT',
          active: true,
          precision: {
            amount: 0.01,
            price: 0.01,
          },
          limits: {
            amount: {
              min: 0.01,
              max: 1000,
            },
            leverage: {
              min: 1,
              max: 100,
            },
          },
          contractSize: 1,
        },
      ],
    });
  });

  describe('placeOrder - Limit Orders', () => {
    it('should place a limit BUY order successfully', async () => {
      // Mock API response
      mockTradingXhr.post.mockResolvedValueOnce({
        data: {
          result: {
            order_id: 'order_123456',
            client_order_id: 'client_abc',
            sub_account_id: 'test_subaccount_456',
            instrument_id: 'BTC_USDT_Perp',
            side: 'BUY',
            type: 'LIMIT',
            time_in_force: 'GOOD_TILL_TIME',
            price: '50000.0',
            size: '0.1',
            filled_size: '0',
            remaining_size: '0.1',
            average_fill_price: '0',
            status: 'OPEN',
            post_only: false,
            reduce_only: false,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'BTC_USDT_Perp',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 50000,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);
      expect(orderIds[0]).toBe('order_123456');

      // Verify API call
      expect(mockTradingXhr.post).toHaveBeenCalledTimes(1);
      expect(mockTradingXhr.post).toHaveBeenCalledWith('/lite/v1/create_order', {
        sub_account_id: 'test_subaccount_456',
        instrument_id: 'BTC_USDT_Perp',
        side: 'BUY',
        type: 'LIMIT',
        time_in_force: 'GOOD_TILL_TIME',
        size: '0.1',
        price: '50000',
        post_only: false,
        reduce_only: false,
      });
    });

    it('should place a limit SELL order successfully', async () => {
      mockTradingXhr.post.mockResolvedValueOnce({
        data: {
          result: {
            order_id: 'order_789012',
            sub_account_id: 'test_subaccount_456',
            instrument_id: 'ETH_USDT_Perp',
            side: 'SELL',
            type: 'LIMIT',
            time_in_force: 'GOOD_TILL_TIME',
            price: '3000.0',
            size: '1.0',
            filled_size: '0',
            remaining_size: '1.0',
            average_fill_price: '0',
            status: 'OPEN',
            post_only: false,
            reduce_only: false,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'ETH_USDT_Perp',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        amount: 1.0,
        price: 3000,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);
      expect(orderIds[0]).toBe('order_789012');

      // Verify API call
      const callArgs = mockTradingXhr.post.mock.calls[0];
      expect(callArgs[0]).toBe('/lite/v1/create_order');
      expect(callArgs[1].side).toBe('SELL');
      expect(callArgs[1].type).toBe('LIMIT');
    });

    it('should place a POST_ONLY limit order', async () => {
      mockTradingXhr.post.mockResolvedValueOnce({
        data: {
          result: {
            order_id: 'order_po_123',
            sub_account_id: 'test_subaccount_456',
            instrument_id: 'BTC_USDT_Perp',
            side: 'BUY',
            type: 'LIMIT',
            time_in_force: 'GOOD_TILL_TIME',
            price: '49000.0',
            size: '0.05',
            filled_size: '0',
            remaining_size: '0.05',
            status: 'OPEN',
            post_only: true,
            reduce_only: false,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'BTC_USDT_Perp',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.05,
        price: 49000,
        timeInForce: OrderTimeInForce.PostOnly,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);

      // Verify post_only flag is set
      const callArgs = mockTradingXhr.post.mock.calls[0][1];
      expect(callArgs.post_only).toBe(true);
    });

    it('should place an IOC limit order', async () => {
      mockTradingXhr.post.mockResolvedValueOnce({
        data: {
          result: {
            order_id: 'order_ioc_456',
            sub_account_id: 'test_subaccount_456',
            instrument_id: 'ETH_USDT_Perp',
            side: 'SELL',
            type: 'LIMIT',
            time_in_force: 'IMMEDIATE_OR_CANCEL',
            price: '3100.0',
            size: '0.5',
            filled_size: '0.5',
            remaining_size: '0',
            status: 'FILLED',
            post_only: false,
            reduce_only: false,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'ETH_USDT_Perp',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        amount: 0.5,
        price: 3100,
        timeInForce: OrderTimeInForce.ImmediateOrCancel,
      });

      expect(orderIds).toBeDefined();
      const callArgs = mockTradingXhr.post.mock.calls[0][1];
      expect(callArgs.time_in_force).toBe('IMMEDIATE_OR_CANCEL');
    });

    it('should place a reduce-only order', async () => {
      mockTradingXhr.post.mockResolvedValueOnce({
        data: {
          result: {
            order_id: 'order_reduce_789',
            sub_account_id: 'test_subaccount_456',
            instrument_id: 'BTC_USDT_Perp',
            side: 'SELL',
            type: 'LIMIT',
            time_in_force: 'GOOD_TILL_TIME',
            price: '51000.0',
            size: '0.1',
            filled_size: '0',
            remaining_size: '0.1',
            status: 'OPEN',
            post_only: false,
            reduce_only: true,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'BTC_USDT_Perp',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        amount: 0.1,
        price: 51000,
        reduceOnly: true,
      });

      expect(orderIds).toBeDefined();
      const callArgs = mockTradingXhr.post.mock.calls[0][1];
      expect(callArgs.reduce_only).toBe(true);
    });

    it('should throw error if market not found', async () => {
      await expect(
        exchange.placeOrder({
          symbol: 'INVALID_USDT_Perp',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 1,
          price: 100,
        })
      ).rejects.toThrow('Market INVALID_USDT_Perp not found');
    });

    it('should handle API errors gracefully', async () => {
      mockTradingXhr.post.mockRejectedValueOnce(
        new Error('Insufficient margin')
      );

      await expect(
        exchange.placeOrder({
          symbol: 'BTC_USDT_Perp',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 0.1,
          price: 50000,
        })
      ).rejects.toThrow('Insufficient margin');
    });
  });

  describe('placeOrder - Market Orders', () => {
    it('should place a market BUY order successfully', async () => {
      mockTradingXhr.post.mockResolvedValueOnce({
        data: {
          result: {
            order_id: 'order_market_123',
            sub_account_id: 'test_subaccount_456',
            instrument_id: 'BTC_USDT_Perp',
            side: 'BUY',
            type: 'MARKET',
            time_in_force: 'GOOD_TILL_TIME',
            price: '0',
            size: '0.05',
            filled_size: '0.05',
            remaining_size: '0',
            average_fill_price: '50123.45',
            status: 'FILLED',
            post_only: false,
            reduce_only: false,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'BTC_USDT_Perp',
        type: OrderType.Market,
        side: OrderSide.Buy,
        amount: 0.05,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);

      // Verify API call - market orders should not include price
      const callArgs = mockTradingXhr.post.mock.calls[0][1];
      expect(callArgs.type).toBe('MARKET');
      expect(callArgs.price).toBeUndefined();
    });

    it('should place a market SELL order successfully', async () => {
      mockTradingXhr.post.mockResolvedValueOnce({
        data: {
          result: {
            order_id: 'order_market_456',
            sub_account_id: 'test_subaccount_456',
            instrument_id: 'ETH_USDT_Perp',
            side: 'SELL',
            type: 'MARKET',
            time_in_force: 'GOOD_TILL_TIME',
            price: '0',
            size: '1.0',
            filled_size: '1.0',
            remaining_size: '0',
            average_fill_price: '2987.65',
            status: 'FILLED',
            post_only: false,
            reduce_only: false,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'ETH_USDT_Perp',
        type: OrderType.Market,
        side: OrderSide.Sell,
        amount: 1.0,
      });

      expect(orderIds).toBeDefined();
      const callArgs = mockTradingXhr.post.mock.calls[0][1];
      expect(callArgs.type).toBe('MARKET');
      expect(callArgs.side).toBe('SELL');
    });
  });

  describe('fetchOrders', () => {
    it('should fetch all open orders', async () => {
      // Mock API response
      mockTradingXhr.post.mockResolvedValueOnce({
        data: {
          result: [
            {
              order_id: 'order1',
              client_order_id: 'client1',
              sub_account_id: 'test_subaccount_456',
              instrument_id: 'BTC_USDT_Perp',
              side: 'BUY',
              type: 'LIMIT',
              time_in_force: 'GOOD_TILL_TIME',
              price: '50000.0',
              size: '0.1',
              filled_size: '0',
              remaining_size: '0.1',
              average_fill_price: '0',
              status: 'OPEN',
              post_only: false,
              reduce_only: false,
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
            },
            {
              order_id: 'order2',
              sub_account_id: 'test_subaccount_456',
              instrument_id: 'ETH_USDT_Perp',
              side: 'SELL',
              type: 'LIMIT',
              time_in_force: 'GOOD_TILL_TIME',
              price: '3000.0',
              size: '1.0',
              filled_size: '0.5',
              remaining_size: '0.5',
              average_fill_price: '3000.0',
              status: 'OPEN',
              post_only: false,
              reduce_only: false,
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
            },
          ],
        },
      });

      const orders = await exchange.fetchOrders();

      expect(orders).toBeDefined();
      expect(orders.length).toBe(2);

      // Verify first order
      expect(orders[0].id).toBe('order1');
      expect(orders[0].symbol).toBe('BTC_USDT_Perp');
      expect(orders[0].side).toBe(OrderSide.Buy);
      expect(orders[0].type).toBe(OrderType.Limit);
      expect(orders[0].price).toBe(50000);
      expect(orders[0].amount).toBe(0.1);
      expect(orders[0].filled).toBe(0);
      expect(orders[0].remaining).toBe(0.1);
      expect(orders[0].status).toBe(OrderStatus.Open);
      expect(orders[0].reduceOnly).toBe(false);

      // Verify second order (partially filled)
      expect(orders[1].id).toBe('order2');
      expect(orders[1].filled).toBe(0.5);
      expect(orders[1].remaining).toBe(0.5);
    });

    it('should fetch orders for a specific symbol', async () => {
      mockTradingXhr.post.mockResolvedValueOnce({
        data: {
          result: [
            {
              order_id: 'order_btc',
              sub_account_id: 'test_subaccount_456',
              instrument_id: 'BTC_USDT_Perp',
              side: 'BUY',
              type: 'LIMIT',
              time_in_force: 'GOOD_TILL_TIME',
              price: '50000.0',
              size: '0.1',
              filled_size: '0',
              remaining_size: '0.1',
              status: 'OPEN',
              post_only: false,
              reduce_only: false,
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
            },
          ],
        },
      });

      const orders = await exchange.fetchOrders(['BTC_USDT_Perp']);

      expect(orders).toBeDefined();
      expect(orders.length).toBe(1);
      expect(orders[0].symbol).toBe('BTC_USDT_Perp');

      // Verify API was called with instrument_id filter
      expect(mockTradingXhr.post).toHaveBeenCalledWith('/lite/v1/open_orders', {
        sub_account_id: 'test_subaccount_456',
        instrument_id: 'BTC_USDT_Perp',
      });
    });

    it('should return empty array when no orders found', async () => {
      mockTradingXhr.post.mockResolvedValueOnce({
        data: { result: [] },
      });

      const orders = await exchange.fetchOrders();

      expect(orders).toBeDefined();
      expect(orders.length).toBe(0);
    });

    it('should map order statuses correctly', async () => {
      mockTradingXhr.post.mockResolvedValueOnce({
        data: {
          result: [
            {
              order_id: 'order_pending',
              sub_account_id: 'test_subaccount_456',
              instrument_id: 'BTC_USDT_Perp',
              side: 'BUY',
              type: 'LIMIT',
              time_in_force: 'GOOD_TILL_TIME',
              price: '50000.0',
              size: '0.1',
              filled_size: '0',
              remaining_size: '0.1',
              status: 'PENDING',
              post_only: false,
              reduce_only: false,
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
            },
            {
              order_id: 'order_filled',
              sub_account_id: 'test_subaccount_456',
              instrument_id: 'BTC_USDT_Perp',
              side: 'SELL',
              type: 'LIMIT',
              time_in_force: 'GOOD_TILL_TIME',
              price: '51000.0',
              size: '0.1',
              filled_size: '0.1',
              remaining_size: '0',
              status: 'FILLED',
              post_only: false,
              reduce_only: false,
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
            },
          ],
        },
      });

      const orders = await exchange.fetchOrders();

      expect(orders[0].status).toBe(OrderStatus.Creating); // PENDING
      expect(orders[1].status).toBe(OrderStatus.Closed); // FILLED
    });

    it('should handle API errors gracefully', async () => {
      mockTradingXhr.post.mockRejectedValueOnce(new Error('API error'));

      await expect(exchange.fetchOrders()).rejects.toThrow('API error');
    });
  });

  describe('cancelOrder', () => {
    it('should cancel an order successfully', async () => {
      mockTradingXhr.post.mockResolvedValueOnce({
        data: { result: { success: true } },
      });

      const order = {
        id: 'order_to_cancel',
        symbol: 'BTC_USDT_Perp',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        price: 50000,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: false,
        lastUpdate: Date.now(),
      };

      await exchange.cancelOrder(order);

      expect(mockTradingXhr.post).toHaveBeenCalledTimes(1);
      expect(mockTradingXhr.post).toHaveBeenCalledWith('/lite/v1/cancel_order', {
        sub_account_id: 'test_subaccount_456',
        order_id: 'order_to_cancel',
        instrument_id: 'BTC_USDT_Perp',
      });
    });

    it('should throw error if market not found', async () => {
      const order = {
        id: 'order1',
        symbol: 'INVALID_USDT_Perp',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        price: 50000,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: false,
        lastUpdate: Date.now(),
      };

      await expect(exchange.cancelOrder(order)).rejects.toThrow(
        'Market INVALID_USDT_Perp not found'
      );
    });

    it('should handle API errors gracefully', async () => {
      mockTradingXhr.post.mockRejectedValueOnce(new Error('Order not found'));

      const order = {
        id: 'nonexistent_order',
        symbol: 'BTC_USDT_Perp',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        price: 50000,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: false,
        lastUpdate: Date.now(),
      };

      await expect(exchange.cancelOrder(order)).rejects.toThrow('Order not found');
    });
  });

  describe('cancelOrders', () => {
    it('should cancel multiple orders successfully', async () => {
      mockTradingXhr.post.mockResolvedValue({
        data: { result: { success: true } },
      });

      const orders = [
        {
          id: 'order1',
          symbol: 'BTC_USDT_Perp',
          side: OrderSide.Buy,
          type: OrderType.Limit,
          status: OrderStatus.Open,
          price: 50000,
          amount: 0.1,
          filled: 0,
          remaining: 0.1,
          reduceOnly: false,
          lastUpdate: Date.now(),
        },
        {
          id: 'order2',
          symbol: 'ETH_USDT_Perp',
          side: OrderSide.Sell,
          type: OrderType.Limit,
          status: OrderStatus.Open,
          price: 3000,
          amount: 1.0,
          filled: 0,
          remaining: 1.0,
          reduceOnly: false,
          lastUpdate: Date.now(),
        },
      ];

      await exchange.cancelOrders(orders);

      expect(mockTradingXhr.post).toHaveBeenCalledTimes(2);
    });

    it('should continue cancelling even if one order fails', async () => {
      let callCount = 0;
      mockTradingXhr.post.mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.reject(new Error('First order failed'));
        }
        return Promise.resolve({ data: { result: { success: true } } });
      });

      const orders = [
        {
          id: 'order1',
          symbol: 'BTC_USDT_Perp',
          side: OrderSide.Buy,
          type: OrderType.Limit,
          status: OrderStatus.Open,
          price: 50000,
          amount: 0.1,
          filled: 0,
          remaining: 0.1,
          reduceOnly: false,
          lastUpdate: Date.now(),
        },
        {
          id: 'order2',
          symbol: 'ETH_USDT_Perp',
          side: OrderSide.Sell,
          type: OrderType.Limit,
          status: OrderStatus.Open,
          price: 3000,
          amount: 1.0,
          filled: 0,
          remaining: 1.0,
          reduceOnly: false,
          lastUpdate: Date.now(),
        },
      ];

      // Should not throw even though first order failed
      await exchange.cancelOrders(orders);

      // Both orders should have been attempted
      expect(mockTradingXhr.post).toHaveBeenCalledTimes(2);
    });
  });

  describe('cancelAllOrders', () => {
    it('should cancel all orders successfully', async () => {
      mockTradingXhr.post.mockResolvedValueOnce({
        data: { result: { success: true } },
      });

      await exchange.cancelAllOrders();

      expect(mockTradingXhr.post).toHaveBeenCalledTimes(1);
      expect(mockTradingXhr.post).toHaveBeenCalledWith('/lite/v1/cancel_all_orders', {
        sub_account_id: 'test_subaccount_456',
      });
    });

    it('should handle API errors', async () => {
      mockTradingXhr.post.mockRejectedValueOnce(new Error('Failed to cancel'));

      await expect(exchange.cancelAllOrders()).rejects.toThrow('Failed to cancel');
    });
  });

  describe('cancelSymbolOrders', () => {
    it('should cancel all orders for a specific symbol', async () => {
      mockTradingXhr.post.mockResolvedValueOnce({
        data: { result: { success: true } },
      });

      await exchange.cancelSymbolOrders('BTC_USDT_Perp');

      expect(mockTradingXhr.post).toHaveBeenCalledTimes(1);
      expect(mockTradingXhr.post).toHaveBeenCalledWith('/lite/v1/cancel_all_orders', {
        sub_account_id: 'test_subaccount_456',
        instrument_id: 'BTC_USDT_Perp',
      });
    });

    it('should throw error if market not found', async () => {
      await expect(
        exchange.cancelSymbolOrders('INVALID_USDT_Perp')
      ).rejects.toThrow('Market INVALID_USDT_Perp not found');
    });
  });

  describe('Complete Order Lifecycle', () => {
    it('should place, fetch, and cancel an order successfully', async () => {
      // Step 1: Place order
      mockTradingXhr.post.mockResolvedValueOnce({
        data: {
          result: {
            order_id: 'lifecycle_order_123',
            sub_account_id: 'test_subaccount_456',
            instrument_id: 'BTC_USDT_Perp',
            side: 'BUY',
            type: 'LIMIT',
            time_in_force: 'GOOD_TILL_TIME',
            price: '50000.0',
            size: '0.1',
            filled_size: '0',
            remaining_size: '0.1',
            status: 'OPEN',
            post_only: false,
            reduce_only: false,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'BTC_USDT_Perp',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 50000,
      });

      expect(orderIds[0]).toBe('lifecycle_order_123');

      // Step 2: Fetch orders
      mockTradingXhr.post.mockResolvedValueOnce({
        data: {
          result: [
            {
              order_id: 'lifecycle_order_123',
              sub_account_id: 'test_subaccount_456',
              instrument_id: 'BTC_USDT_Perp',
              side: 'BUY',
              type: 'LIMIT',
              time_in_force: 'GOOD_TILL_TIME',
              price: '50000.0',
              size: '0.1',
              filled_size: '0',
              remaining_size: '0.1',
              status: 'OPEN',
              post_only: false,
              reduce_only: false,
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
            },
          ],
        },
      });

      const orders = await exchange.fetchOrders();
      expect(orders.length).toBe(1);
      expect(orders[0].id).toBe('lifecycle_order_123');

      // Step 3: Cancel the order
      mockTradingXhr.post.mockResolvedValueOnce({
        data: { result: { success: true } },
      });

      await exchange.cancelOrder(orders[0]);

      expect(mockTradingXhr.post).toHaveBeenCalledWith('/lite/v1/cancel_order', {
        sub_account_id: 'test_subaccount_456',
        order_id: 'lifecycle_order_123',
        instrument_id: 'BTC_USDT_Perp',
      });
    });
  });
});
