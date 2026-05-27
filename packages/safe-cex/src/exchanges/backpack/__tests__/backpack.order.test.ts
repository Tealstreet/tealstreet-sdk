import { describe, it, expect, beforeEach, vi } from 'vitest';
import { BackpackExchange } from '../backpack.exchange';
import { DefaultStore } from '../../../store/store.base';
import { OrderSide, OrderType, OrderTimeInForce, OrderStatus, ExchangeOptions } from '../../../types';

/**
 * Unit tests for Backpack order management (placeOrder, fetchOrders, cancelOrder)
 * Uses mocks to avoid real API calls
 */
describe('BackpackExchange - Order Management (with mocks)', () => {
  let exchange: BackpackExchange;
  let store: DefaultStore;
  let mockXhr: any;

  beforeEach(() => {
    // Create exchange with test options
    const options: ExchangeOptions = {
      key: 'test_api_key_base64',
      secret: 'test_secret_base64',
      testnet: false,
    };

    store = new DefaultStore();
    exchange = new BackpackExchange('backpack', options, store);

    // Mock the XHR client
    mockXhr = {
      get: vi.fn(),
      post: vi.fn(),
      delete: vi.fn(),
    };

    // Replace the exchange's xhr with our mock
    exchange.xhr = mockXhr as any;

    // Set up mock markets in store
    store.update({
      markets: [
        {
          id: 'SOL_USDC',
          symbol: 'SOL_USDC',
          base: 'SOL',
          quote: 'USDC',
          active: true,
          precision: {
            amount: 0.01,
            price: 0.01,
          },
          limits: {
            amount: {
              min: 0.01,
              max: 10000,
            },
            leverage: {
              min: 1,
              max: 10,
            },
          },
          contractSize: 1,
          takerFee: 0.0006,
          makerFee: 0.0002,
        },
        {
          id: 'BTC_USDC',
          symbol: 'BTC_USDC',
          base: 'BTC',
          quote: 'USDC',
          active: true,
          precision: {
            amount: 0.0001,
            price: 0.01,
          },
          limits: {
            amount: {
              min: 0.0001,
              max: 100,
            },
            leverage: {
              min: 1,
              max: 10,
            },
          },
          contractSize: 1,
          takerFee: 0.0006,
          makerFee: 0.0002,
        },
      ],
    });
  });

  describe('placeOrder - Limit Orders', () => {
    it('should place a limit BUY order successfully', async () => {
      // Mock API response
      mockXhr.post.mockResolvedValueOnce({
        data: {
          id: 'order_123456',
          clientId: 'client_abc',
          symbol: 'SOL_USDC',
          side: 'Bid',
          orderType: 'Limit',
          price: '150.00',
          quantity: '10',
          timeInForce: 'GTC',
          selfTradePrevention: 'RejectTaker',
          status: 'New',
          createdAt: Date.now(),
          executedQuantity: '0',
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'SOL_USDC',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 10,
        price: 150,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);
      expect(orderIds[0]).toBe('order_123456');

      // Verify API call
      expect(mockXhr.post).toHaveBeenCalledTimes(1);
      expect(mockXhr.post).toHaveBeenCalledWith('/api/v1/order', {
        symbol: 'SOL_USDC',
        side: 'Bid',
        orderType: 'Limit',
        quantity: '10.00', // Formatted to 2 decimal places based on market precision
        price: '150.00',
        timeInForce: 'GTC',
        postOnly: false,
      });
    });

    it('should place a limit SELL order successfully', async () => {
      mockXhr.post.mockResolvedValueOnce({
        data: {
          id: 'order_789012',
          symbol: 'BTC_USDC',
          side: 'Ask',
          orderType: 'Limit',
          price: '45000.00',
          quantity: '0.5',
          timeInForce: 'GTC',
          status: 'New',
          createdAt: Date.now(),
          executedQuantity: '0',
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'BTC_USDC',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        amount: 0.5,
        price: 45000,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);
      expect(orderIds[0]).toBe('order_789012');

      // Verify API call
      expect(mockXhr.post).toHaveBeenCalledWith('/api/v1/order', {
        symbol: 'BTC_USDC',
        side: 'Ask',
        orderType: 'Limit',
        quantity: '0.5000', // Formatted to 4 decimal places based on BTC market precision
        price: '45000.00',
        timeInForce: 'GTC',
        postOnly: false,
      });
    });

    it('should place a POST_ONLY limit order', async () => {
      mockXhr.post.mockResolvedValueOnce({
        data: {
          id: 'order_po_123',
          symbol: 'SOL_USDC',
          side: 'Bid',
          orderType: 'Limit',
          price: '148.00',
          quantity: '5',
          timeInForce: 'GTC',
          postOnly: true,
          status: 'New',
          createdAt: Date.now(),
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'SOL_USDC',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 5,
        price: 148,
        timeInForce: OrderTimeInForce.PostOnly,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);

      // Verify postOnly is true and timeInForce is 'GTC' (Backpack uses postOnly field instead of timeInForce PO)
      const callArgs = mockXhr.post.mock.calls[0][1];
      expect(callArgs.postOnly).toBe(true);
      expect(callArgs.timeInForce).toBe('GTC');
    });

    it('should place an IOC limit order', async () => {
      mockXhr.post.mockResolvedValueOnce({
        data: {
          id: 'order_ioc_456',
          symbol: 'SOL_USDC',
          side: 'Ask',
          orderType: 'Limit',
          price: '152.00',
          quantity: '3',
          timeInForce: 'IOC',
          status: 'Filled',
          createdAt: Date.now(),
          executedQuantity: '3',
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'SOL_USDC',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        amount: 3,
        price: 152,
        timeInForce: OrderTimeInForce.ImmediateOrCancel,
      });

      expect(orderIds).toBeDefined();
      const callArgs = mockXhr.post.mock.calls[0][1];
      expect(callArgs.timeInForce).toBe('IOC');
    });

    it('should throw error if market not found', async () => {
      await expect(
        exchange.placeOrder({
          symbol: 'INVALID_USDC',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 1,
          price: 100,
        })
      ).rejects.toThrow('Market not found: INVALID_USDC');
    });

    it('should handle API errors gracefully', async () => {
      mockXhr.post.mockRejectedValueOnce(
        new Error('Insufficient balance')
      );

      await expect(
        exchange.placeOrder({
          symbol: 'SOL_USDC',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 10,
          price: 150,
        })
      ).rejects.toThrow('Insufficient balance');
    });
  });

  describe('placeOrder - Market Orders', () => {
    it('should place a market BUY order successfully', async () => {
      mockXhr.post.mockResolvedValueOnce({
        data: {
          id: 'order_market_123',
          symbol: 'SOL_USDC',
          side: 'Bid',
          orderType: 'Market',
          price: '0', // Market orders may have 0 price
          quantity: '5',
          timeInForce: 'GTC',
          status: 'Filled',
          createdAt: Date.now(),
          executedQuantity: '5',
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'SOL_USDC',
        type: OrderType.Market,
        side: OrderSide.Buy,
        amount: 5,
      });

      expect(orderIds).toBeDefined();
      expect(orderIds.length).toBe(1);

      // Verify API call - market orders should not include price
      const callArgs = mockXhr.post.mock.calls[0][1];
      expect(callArgs.orderType).toBe('Market');
      expect(callArgs.price).toBeUndefined();
    });

    it('should place a market SELL order successfully', async () => {
      mockXhr.post.mockResolvedValueOnce({
        data: {
          id: 'order_market_456',
          symbol: 'BTC_USDC',
          side: 'Ask',
          orderType: 'Market',
          price: '0',
          quantity: '0.1',
          timeInForce: 'GTC',
          status: 'Filled',
          createdAt: Date.now(),
          executedQuantity: '0.1',
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'BTC_USDC',
        type: OrderType.Market,
        side: OrderSide.Sell,
        amount: 0.1,
      });

      expect(orderIds).toBeDefined();
      const callArgs = mockXhr.post.mock.calls[0][1];
      expect(callArgs.orderType).toBe('Market');
      expect(callArgs.side).toBe('Ask');
    });
  });

  describe('fetchOrders', () => {
    it('should fetch orders for all symbols', async () => {
      // Mock API response - Backpack returns all open orders in a single call to /api/v1/orders
      mockXhr.get.mockResolvedValueOnce({
        data: [
          {
            id: 'order1',
            clientId: 'client1',
            symbol: 'SOL_USDC',
            side: 'Bid',
            orderType: 'Limit',
            price: '150.00',
            quantity: '10',
            timeInForce: 'GTC',
            status: 'New',
            createdAt: Date.now(),
            executedQuantity: '0',
          },
          {
            id: 'order2',
            clientId: 'client2',
            symbol: 'SOL_USDC',
            side: 'Ask',
            orderType: 'Limit',
            price: '155.00',
            quantity: '5',
            timeInForce: 'GTC',
            status: 'PartiallyFilled',
            createdAt: Date.now(),
            executedQuantity: '2',
          },
          {
            id: 'order3',
            symbol: 'BTC_USDC',
            side: 'Bid',
            orderType: 'Limit',
            price: '45000.00',
            quantity: '0.5',
            timeInForce: 'GTC',
            status: 'New',
            createdAt: Date.now(),
            executedQuantity: '0',
          },
        ],
      });

      const orders = await exchange.fetchOrders();

      expect(orders).toBeDefined();
      expect(orders.length).toBe(3);

      // Verify first order
      expect(orders[0].id).toBe('order1');
      expect(orders[0].symbol).toBe('SOL_USDC');
      expect(orders[0].side).toBe(OrderSide.Buy);
      expect(orders[0].type).toBe(OrderType.Limit);
      expect(orders[0].price).toBe(150);
      expect(orders[0].amount).toBe(10);
      expect(orders[0].filled).toBe(0);
      expect(orders[0].remaining).toBe(10);
      expect(orders[0].status).toBe(OrderStatus.Open);

      // Verify second order (partially filled)
      expect(orders[1].id).toBe('order2');
      expect(orders[1].filled).toBe(2);
      expect(orders[1].remaining).toBe(3);
      expect(orders[1].status).toBe(OrderStatus.Open);

      // Verify third order
      expect(orders[2].id).toBe('order3');
      expect(orders[2].symbol).toBe('BTC_USDC');
    });

    it('should return empty array when no orders found', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      const orders = await exchange.fetchOrders();

      expect(orders).toBeDefined();
      expect(orders.length).toBe(0);
    });

    it('should handle API errors gracefully', async () => {
      mockXhr.get.mockRejectedValue(new Error('API error'));

      const orders = await exchange.fetchOrders();

      // Should return empty array on error
      expect(orders).toBeDefined();
      expect(orders.length).toBe(0);
    });

    it('should map order statuses correctly', async () => {
      mockXhr.get.mockResolvedValueOnce({
        data: [
          {
            id: 'order_new',
            symbol: 'SOL_USDC',
            side: 'Bid',
            orderType: 'Limit',
            price: '150.00',
            quantity: '10',
            timeInForce: 'GTC',
            status: 'New',
            createdAt: Date.now(),
            executedQuantity: '0',
          },
          {
            id: 'order_filled',
            symbol: 'SOL_USDC',
            side: 'Ask',
            orderType: 'Limit',
            price: '155.00',
            quantity: '5',
            timeInForce: 'GTC',
            status: 'Filled',
            createdAt: Date.now(),
            executedQuantity: '5',
          },
          {
            id: 'order_cancelled',
            symbol: 'SOL_USDC',
            side: 'Bid',
            orderType: 'Limit',
            price: '145.00',
            quantity: '3',
            timeInForce: 'GTC',
            status: 'Cancelled',
            createdAt: Date.now(),
            executedQuantity: '0',
          },
        ],
      });

      const orders = await exchange.fetchOrders();

      expect(orders[0].status).toBe(OrderStatus.Open); // New
      expect(orders[1].status).toBe(OrderStatus.Closed); // Filled
      expect(orders[2].status).toBe(OrderStatus.Canceled); // Cancelled
    });
  });

  describe('cancelOrder', () => {
    it('should cancel an order successfully', async () => {
      mockXhr.delete.mockResolvedValueOnce({ data: { success: true } });

      const order = {
        id: 'order_to_cancel',
        symbol: 'SOL_USDC',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        price: 150,
        amount: 10,
        filled: 0,
        remaining: 10,
        reduceOnly: false,
        lastUpdate: Date.now(),
      };

      await exchange.cancelOrder(order);

      expect(mockXhr.delete).toHaveBeenCalledTimes(1);
      expect(mockXhr.delete).toHaveBeenCalledWith('/api/v1/order', {
        data: {
          symbol: 'SOL_USDC',
          orderId: 'order_to_cancel',
        },
      });
    });

    it('should throw error if market not found', async () => {
      const order = {
        id: 'order1',
        symbol: 'INVALID_USDC',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        price: 150,
        amount: 10,
        filled: 0,
        remaining: 10,
        reduceOnly: false,
        lastUpdate: Date.now(),
      };

      await expect(exchange.cancelOrder(order)).rejects.toThrow(
        'Market not found: INVALID_USDC'
      );
    });

    it('should handle API errors gracefully', async () => {
      mockXhr.delete.mockRejectedValueOnce(new Error('Order not found'));

      const order = {
        id: 'nonexistent_order',
        symbol: 'SOL_USDC',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        price: 150,
        amount: 10,
        filled: 0,
        remaining: 10,
        reduceOnly: false,
        lastUpdate: Date.now(),
      };

      await expect(exchange.cancelOrder(order)).rejects.toThrow('Order not found');
    });
  });

  describe('cancelOrders', () => {
    it('should cancel multiple orders successfully', async () => {
      mockXhr.delete.mockResolvedValue({ data: { success: true } });

      const orders = [
        {
          id: 'order1',
          symbol: 'SOL_USDC',
          side: OrderSide.Buy,
          type: OrderType.Limit,
          status: OrderStatus.Open,
          price: 150,
          amount: 10,
          filled: 0,
          remaining: 10,
          reduceOnly: false,
          lastUpdate: Date.now(),
        },
        {
          id: 'order2',
          symbol: 'BTC_USDC',
          side: OrderSide.Sell,
          type: OrderType.Limit,
          status: OrderStatus.Open,
          price: 45000,
          amount: 0.5,
          filled: 0,
          remaining: 0.5,
          reduceOnly: false,
          lastUpdate: Date.now(),
        },
      ];

      await exchange.cancelOrders(orders);

      expect(mockXhr.delete).toHaveBeenCalledTimes(2);
    });

    it('should continue cancelling even if one order fails', async () => {
      let callCount = 0;
      mockXhr.delete.mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.reject(new Error('First order failed'));
        }
        return Promise.resolve({ data: { success: true } });
      });

      const orders = [
        {
          id: 'order1',
          symbol: 'SOL_USDC',
          side: OrderSide.Buy,
          type: OrderType.Limit,
          status: OrderStatus.Open,
          price: 150,
          amount: 10,
          filled: 0,
          remaining: 10,
          reduceOnly: false,
          lastUpdate: Date.now(),
        },
        {
          id: 'order2',
          symbol: 'BTC_USDC',
          side: OrderSide.Sell,
          type: OrderType.Limit,
          status: OrderStatus.Open,
          price: 45000,
          amount: 0.5,
          filled: 0,
          remaining: 0.5,
          reduceOnly: false,
          lastUpdate: Date.now(),
        },
      ];

      // Should not throw even though first order failed
      await exchange.cancelOrders(orders);

      // Both orders should have been attempted
      expect(mockXhr.delete).toHaveBeenCalledTimes(2);
    });
  });

  describe('cancelSymbolOrders', () => {
    it('should cancel all orders for a specific symbol', async () => {
      mockXhr.delete.mockResolvedValueOnce({ data: { success: true } });

      await exchange.cancelSymbolOrders('SOL_USDC');

      expect(mockXhr.delete).toHaveBeenCalledTimes(1);
      expect(mockXhr.delete).toHaveBeenCalledWith('/api/v1/orders', {
        data: { symbol: 'SOL_USDC' },
      });
    });

    it('should handle market not found gracefully', async () => {
      // Should not throw for invalid market
      await exchange.cancelSymbolOrders('INVALID_USDC');

      // Should not make API call
      expect(mockXhr.delete).not.toHaveBeenCalled();
    });
  });

  describe('cancelAllOrders', () => {
    it('should cancel all orders for all markets', async () => {
      mockXhr.delete.mockResolvedValue({ data: { success: true } });

      await exchange.cancelAllOrders();

      // Should call delete for each market
      expect(mockXhr.delete).toHaveBeenCalledTimes(2);
      expect(mockXhr.delete).toHaveBeenCalledWith('/api/v1/orders', {
        data: { symbol: 'SOL_USDC' },
      });
      expect(mockXhr.delete).toHaveBeenCalledWith('/api/v1/orders', {
        data: { symbol: 'BTC_USDC' },
      });
    });

    it('should continue with other markets if one fails', async () => {
      let callCount = 0;
      mockXhr.delete.mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.reject(new Error('Failed to cancel'));
        }
        return Promise.resolve({ data: { success: true } });
      });

      await exchange.cancelAllOrders();

      // Should attempt both markets
      expect(mockXhr.delete).toHaveBeenCalledTimes(2);
    });
  });

  describe('updateOrder - TP/SL Orders', () => {
    it('should update a StopLoss order successfully', async () => {
      // Add an existing StopLoss order to the store
      const existingOrder = {
        id: 'sl_order_123',
        symbol: 'SOL_USDC',
        side: OrderSide.Sell,
        type: OrderType.StopLoss,
        status: OrderStatus.Open,
        price: 140, // This is the trigger price
        amount: 10,
        filled: 0,
        remaining: 10,
        reduceOnly: true,
        lastUpdate: Date.now(),
        timeInForce: OrderTimeInForce.GoodTillCancel,
      };
      store.addOrUpdateOrder(existingOrder);

      // Add ticker for TP/SL determination
      store.update({
        tickers: [
          {
            id: 'SOL_USDC',
            symbol: 'SOL_USDC',
            bid: 149,
            ask: 151,
            last: 150,
            mark: 150,
            index: 150,
            percentage: 0,
            fundingRate: 0,
            fundingTime: 0,
            volume: 1000,
            quoteVolume: 150000,
            openInterest: 0,
          },
        ],
      });

      // Mock cancel order response
      mockXhr.delete.mockResolvedValueOnce({ data: { success: true } });

      // Mock place order response for the new order
      mockXhr.post.mockResolvedValueOnce({
        data: {
          id: 'sl_order_456',
          symbol: 'SOL_USDC',
          side: 'Ask',
          orderType: 'Market',
          triggerPrice: '135.00',
          quantity: '10',
          status: 'New',
          createdAt: Date.now(),
        },
      });

      const newOrderIds = await exchange.updateOrder({
        order: existingOrder,
        update: { price: 135 }, // Update trigger price from 140 to 135
      });

      expect(newOrderIds).toBeDefined();
      expect(newOrderIds.length).toBe(1);
      expect(newOrderIds[0]).toBe('sl_order_456');

      // Verify cancel was called
      expect(mockXhr.delete).toHaveBeenCalledWith('/api/v1/order', {
        data: {
          symbol: 'SOL_USDC',
          orderId: 'sl_order_123',
        },
      });

      // Verify place order was called with correct parameters for StopLoss
      expect(mockXhr.post).toHaveBeenCalledTimes(1);
      const placeOrderArgs = mockXhr.post.mock.calls[0][1];
      expect(placeOrderArgs.orderType).toBe('Market'); // StopLoss uses Market
      expect(placeOrderArgs.triggerPrice).toBe('135.00'); // Updated trigger price (formatted)
      expect(placeOrderArgs.triggerQuantity).toBe('10.00'); // Uses triggerQuantity for trigger orders (formatted)
      expect(placeOrderArgs.reduceOnly).toBe(true); // StopLoss should be reduceOnly
      expect(placeOrderArgs.quantity).toBeUndefined(); // Should NOT have regular quantity
    });

    it('should update a TakeProfit order successfully', async () => {
      // Add an existing TakeProfit order to the store
      const existingOrder = {
        id: 'tp_order_123',
        symbol: 'SOL_USDC',
        side: OrderSide.Sell,
        type: OrderType.TakeProfit,
        status: OrderStatus.Open,
        price: 160, // This is the trigger price
        amount: 5,
        filled: 0,
        remaining: 5,
        reduceOnly: true,
        lastUpdate: Date.now(),
        timeInForce: OrderTimeInForce.GoodTillCancel,
      };
      store.addOrUpdateOrder(existingOrder);

      // Add ticker for TP/SL determination
      store.update({
        tickers: [
          {
            id: 'SOL_USDC',
            symbol: 'SOL_USDC',
            bid: 149,
            ask: 151,
            last: 150,
            mark: 150,
            index: 150,
            percentage: 0,
            fundingRate: 0,
            fundingTime: 0,
            volume: 1000,
            quoteVolume: 150000,
            openInterest: 0,
          },
        ],
      });

      // Mock cancel order response
      mockXhr.delete.mockResolvedValueOnce({ data: { success: true } });

      // Mock place order response for the new order
      mockXhr.post.mockResolvedValueOnce({
        data: {
          id: 'tp_order_456',
          symbol: 'SOL_USDC',
          side: 'Ask',
          orderType: 'Market',
          triggerPrice: '165.00',
          quantity: '5',
          status: 'New',
          createdAt: Date.now(),
        },
      });

      const newOrderIds = await exchange.updateOrder({
        order: existingOrder,
        update: { price: 165 }, // Update trigger price from 160 to 165
      });

      expect(newOrderIds).toBeDefined();
      expect(newOrderIds.length).toBe(1);
      expect(newOrderIds[0]).toBe('tp_order_456');

      // Verify place order was called with correct parameters for TakeProfit
      const placeOrderArgs = mockXhr.post.mock.calls[0][1];
      expect(placeOrderArgs.orderType).toBe('Market'); // TakeProfit uses Market
      expect(placeOrderArgs.triggerPrice).toBe('165.00'); // Updated trigger price (formatted)
      expect(placeOrderArgs.triggerQuantity).toBe('5.00'); // Uses triggerQuantity (formatted)
      expect(placeOrderArgs.reduceOnly).toBe(true); // TakeProfit should be reduceOnly
    });

    it('should update amount of a StopLoss order', async () => {
      const existingOrder = {
        id: 'sl_order_789',
        symbol: 'SOL_USDC',
        side: OrderSide.Sell,
        type: OrderType.StopLoss,
        status: OrderStatus.Open,
        price: 140,
        amount: 10,
        filled: 0,
        remaining: 10,
        reduceOnly: true,
        lastUpdate: Date.now(),
        timeInForce: OrderTimeInForce.GoodTillCancel,
      };
      store.addOrUpdateOrder(existingOrder);

      store.update({
        tickers: [
          {
            id: 'SOL_USDC',
            symbol: 'SOL_USDC',
            bid: 149,
            ask: 151,
            last: 150,
            mark: 150,
            index: 150,
            percentage: 0,
            fundingRate: 0,
            fundingTime: 0,
            volume: 1000,
            quoteVolume: 150000,
            openInterest: 0,
          },
        ],
      });

      mockXhr.delete.mockResolvedValueOnce({ data: { success: true } });
      mockXhr.post.mockResolvedValueOnce({
        data: {
          id: 'sl_order_new',
          symbol: 'SOL_USDC',
          side: 'Ask',
          orderType: 'Market',
          triggerPrice: '140.00',
          quantity: '15',
          status: 'New',
          createdAt: Date.now(),
        },
      });

      const newOrderIds = await exchange.updateOrder({
        order: existingOrder,
        update: { amount: 15 }, // Update amount from 10 to 15
      });

      expect(newOrderIds).toBeDefined();
      expect(newOrderIds[0]).toBe('sl_order_new');

      const placeOrderArgs = mockXhr.post.mock.calls[0][1];
      expect(placeOrderArgs.triggerQuantity).toBe('15.00'); // Updated amount (formatted)
      expect(placeOrderArgs.triggerPrice).toBe('140.00'); // Price unchanged (formatted)
    });

    it('should throw error if TP/SL order not found in store', async () => {
      const nonExistentOrder = {
        id: 'nonexistent_sl',
        symbol: 'SOL_USDC',
        side: OrderSide.Sell,
        type: OrderType.StopLoss,
        status: OrderStatus.Open,
        price: 140,
        amount: 10,
        filled: 0,
        remaining: 10,
        reduceOnly: true,
        lastUpdate: Date.now(),
        timeInForce: OrderTimeInForce.GoodTillCancel,
      };

      await expect(
        exchange.updateOrder({
          order: nonExistentOrder,
          update: { price: 135 },
        })
      ).rejects.toThrow('Order not found: nonexistent_sl');
    });

    it('should throw error if order has zero or invalid amount', async () => {
      // This tests the case where a trigger order might have amount=0 due to API returning
      // quantity in triggerQuantity field instead of quantity field
      const orderWithZeroAmount = {
        id: 'zero_amount_order',
        symbol: 'SOL_USDC',
        side: OrderSide.Sell,
        type: OrderType.StopLoss,
        status: OrderStatus.Open,
        price: 140,
        amount: 0, // Zero amount - should fail validation
        filled: 0,
        remaining: 0,
        reduceOnly: true,
        lastUpdate: Date.now(),
        timeInForce: OrderTimeInForce.GoodTillCancel,
      };
      store.addOrUpdateOrder(orderWithZeroAmount);

      await expect(
        exchange.updateOrder({
          order: orderWithZeroAmount,
          update: { price: 135 },
        })
      ).rejects.toThrow('Invalid order amount: 0');
    });
  });

  describe('fetchOrders - Trigger Orders', () => {
    it('should correctly map trigger orders using triggerQuantity when available', async () => {
      // Add ticker for TP/SL determination
      store.update({
        tickers: [
          {
            id: 'SOL_USDC',
            symbol: 'SOL_USDC',
            bid: 149,
            ask: 151,
            last: 150,
            mark: 150,
            index: 150,
            percentage: 0,
            fundingRate: 0,
            fundingTime: 0,
            volume: 1000,
            quoteVolume: 150000,
            openInterest: 0,
          },
        ],
      });

      // Mock API response with trigger order that has triggerQuantity instead of quantity
      mockXhr.get.mockResolvedValueOnce({
        data: [
          {
            id: 'trigger_order_1',
            symbol: 'SOL_USDC',
            side: 'Ask',
            orderType: 'Market',
            price: '0', // Market orders may have 0 price
            triggerPrice: '140.00', // This is a trigger/stop order
            quantity: '0', // Regular quantity is 0 for trigger orders
            triggerQuantity: '10', // Amount is in triggerQuantity
            timeInForce: 'GTC',
            status: 'New',
            createdAt: Date.now(),
            executedQuantity: '0',
          },
        ],
      });

      const orders = await exchange.fetchOrders();

      expect(orders).toBeDefined();
      expect(orders.length).toBe(1);

      const order = orders[0];
      expect(order.id).toBe('trigger_order_1');
      expect(order.type).toBe(OrderType.StopLoss); // Sell below current price = StopLoss
      expect(order.price).toBe(140); // Should use triggerPrice
      expect(order.amount).toBe(10); // Should use triggerQuantity, not quantity (which is 0)
      expect(order.reduceOnly).toBe(true);
    });

    it('should fall back to quantity when triggerQuantity is not present', async () => {
      store.update({
        tickers: [
          {
            id: 'SOL_USDC',
            symbol: 'SOL_USDC',
            bid: 149,
            ask: 151,
            last: 150,
            mark: 150,
            index: 150,
            percentage: 0,
            fundingRate: 0,
            fundingTime: 0,
            volume: 1000,
            quoteVolume: 150000,
            openInterest: 0,
          },
        ],
      });

      // Trigger order without triggerQuantity field (some API versions might use quantity)
      mockXhr.get.mockResolvedValueOnce({
        data: [
          {
            id: 'trigger_order_2',
            symbol: 'SOL_USDC',
            side: 'Ask',
            orderType: 'Market',
            price: '0',
            triggerPrice: '160.00', // Above current = TakeProfit
            quantity: '5', // Amount in regular quantity field
            // No triggerQuantity field
            timeInForce: 'GTC',
            status: 'New',
            createdAt: Date.now(),
            executedQuantity: '0',
          },
        ],
      });

      const orders = await exchange.fetchOrders();

      expect(orders.length).toBe(1);
      const order = orders[0];
      expect(order.type).toBe(OrderType.TakeProfit); // Sell above current price = TakeProfit
      expect(order.price).toBe(160);
      expect(order.amount).toBe(5); // Falls back to quantity
    });
  });

  describe('Complete Order Lifecycle', () => {
    it('should place, fetch, and cancel an order successfully', async () => {
      // Step 1: Place order
      mockXhr.post.mockResolvedValueOnce({
        data: {
          id: 'lifecycle_order_123',
          symbol: 'SOL_USDC',
          side: 'Bid',
          orderType: 'Limit',
          price: '150.00',
          quantity: '10',
          timeInForce: 'GTC',
          status: 'New',
          createdAt: Date.now(),
          executedQuantity: '0',
        },
      });

      const orderIds = await exchange.placeOrder({
        symbol: 'SOL_USDC',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 10,
        price: 150,
      });

      expect(orderIds[0]).toBe('lifecycle_order_123');

      // Step 2: Fetch orders
      mockXhr.get.mockResolvedValueOnce({
        data: [
          {
            id: 'lifecycle_order_123',
            symbol: 'SOL_USDC',
            side: 'Bid',
            orderType: 'Limit',
            price: '150.00',
            quantity: '10',
            timeInForce: 'GTC',
            status: 'New',
            createdAt: Date.now(),
            executedQuantity: '0',
          },
        ],
      });
      mockXhr.get.mockResolvedValueOnce({ data: [] }); // BTC_USDC has no orders

      const orders = await exchange.fetchOrders();
      expect(orders.length).toBe(1);
      expect(orders[0].id).toBe('lifecycle_order_123');

      // Step 3: Cancel the order
      mockXhr.delete.mockResolvedValueOnce({ data: { success: true } });

      await exchange.cancelOrder(orders[0]);

      expect(mockXhr.delete).toHaveBeenCalledWith('/api/v1/order', {
        data: {
          symbol: 'SOL_USDC',
          orderId: 'lifecycle_order_123',
        },
      });
    });
  });

  describe('setLeverage', () => {
    beforeEach(() => {
      // Add patch mock for leverage tests
      mockXhr.patch = vi.fn();
    });

    it('should set account leverage limit successfully', async () => {
      mockXhr.patch.mockResolvedValueOnce({ data: {} });
      // Mock positions fetch after setting leverage
      mockXhr.get.mockResolvedValueOnce({ data: [] });

      await exchange.setLeverage('SOL_USDC', 10);

      expect(mockXhr.patch).toHaveBeenCalledWith('/api/v1/account', {
        leverageLimit: '10',
      });
    });

    it('should set leverage with decimal values', async () => {
      mockXhr.patch.mockResolvedValueOnce({ data: {} });
      mockXhr.get.mockResolvedValueOnce({ data: [] });

      await exchange.setLeverage('BTC_USDC', 5);

      expect(mockXhr.patch).toHaveBeenCalledWith('/api/v1/account', {
        leverageLimit: '5',
      });
    });

    it('should emit error when leverage setting fails', async () => {
      const errorSpy = vi.fn();
      exchange.emitter.on('error', errorSpy);

      mockXhr.patch.mockRejectedValueOnce(new Error('API Error: Invalid leverage'));

      await expect(exchange.setLeverage('SOL_USDC', 100)).rejects.toThrow('API Error: Invalid leverage');
      expect(errorSpy).toHaveBeenCalled();
    });

    it('should set leverage regardless of symbol (account-wide setting)', async () => {
      mockXhr.patch.mockResolvedValueOnce({ data: {} });
      mockXhr.get.mockResolvedValueOnce({ data: [] });

      // Setting leverage for any symbol should make the same API call
      await exchange.setLeverage('BTC_USDC', 20);

      expect(mockXhr.patch).toHaveBeenCalledWith('/api/v1/account', {
        leverageLimit: '20',
      });

      mockXhr.patch.mockClear();
      mockXhr.patch.mockResolvedValueOnce({ data: {} });
      mockXhr.get.mockResolvedValueOnce({ data: [] });

      await exchange.setLeverage('SOL_USDC', 20);

      // Same API call regardless of symbol
      expect(mockXhr.patch).toHaveBeenCalledWith('/api/v1/account', {
        leverageLimit: '20',
      });
    });

    it('should update positions with new leverage after setting', async () => {
      mockXhr.patch.mockResolvedValueOnce({ data: {} });
      // Mock positions with actual position data
      mockXhr.get.mockResolvedValueOnce({
        data: [
          {
            symbol: 'SOL_USDC',
            positionId: '123',
            netQuantity: '10',
            netExposureNotional: '1500',
            entryPrice: '150',
            pnlUnrealized: '50',
            estLiquidationPrice: '100',
            imf: '0.1',
          },
        ],
      });

      await exchange.setLeverage('SOL_USDC', 15);

      // Verify positions were fetched
      expect(mockXhr.get).toHaveBeenCalledWith('/api/v1/position');

      // Check that positions in store have the new leverage
      const positions = store.positions;
      expect(positions.length).toBeGreaterThan(0);
      // All positions should have leverage = 15
      for (const position of positions) {
        expect(position.leverage).toBe(15);
      }
    });

    it('should fetch account info and use leverage for positions', async () => {
      // Mock account info fetch
      mockXhr.get.mockResolvedValueOnce({
        data: {
          leverageLimit: '25',
          autoBorrowSettlements: false,
          autoLend: false,
          autoRealizePnl: false,
          autoRepayBorrows: false,
        },
      });
      // Mock positions fetch
      mockXhr.get.mockResolvedValueOnce({ data: [] });

      // Call fetchAccountInfo
      await exchange.fetchAccountInfo();

      // Now fetch positions
      const positions = await exchange.fetchPositions();

      // All fake positions should have leverage = 25 (from account info)
      for (const position of positions) {
        expect(position.leverage).toBe(25);
      }
    });
  });
});
