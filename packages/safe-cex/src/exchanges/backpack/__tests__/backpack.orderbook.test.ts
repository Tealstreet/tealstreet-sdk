/**
 * Unit tests for Backpack listenOrderBook implementation
 * Tests orderbook streaming with snapshot + delta updates
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { calcOrderBookTotal, sortOrderBook } from '../../../utils/orderbook';
import { updateOrderBook } from '../../../utils/updateOrderBook';
import type { OrderBook } from '../../../types';
import type { BackpackWsDepthUpdate } from '../backpack.types';
import { BackpackPublicWebSocket } from '../backpack.ws-public';
import { BackpackExchange } from '../backpack.exchange';
import { DefaultStore } from '../../../store/store.base';

describe('Backpack OrderBook Utils', () => {
  let orderBook: OrderBook;

  beforeEach(() => {
    orderBook = { bids: [], asks: [] };
  });

  describe('updateOrderBook utility', () => {
    it('should add new bid level', () => {
      updateOrderBook(orderBook, 'bids', { price: 100, amount: 1, total: 0 });
      expect(orderBook.bids).toHaveLength(1);
      expect(orderBook.bids[0].price).toBe(100);
      expect(orderBook.bids[0].amount).toBe(1);
    });

    it('should add new ask level', () => {
      updateOrderBook(orderBook, 'asks', { price: 101, amount: 2, total: 0 });
      expect(orderBook.asks).toHaveLength(1);
      expect(orderBook.asks[0].price).toBe(101);
      expect(orderBook.asks[0].amount).toBe(2);
    });

    it('should update existing bid level', () => {
      updateOrderBook(orderBook, 'bids', { price: 100, amount: 1, total: 0 });
      updateOrderBook(orderBook, 'bids', { price: 100, amount: 5, total: 0 });
      expect(orderBook.bids).toHaveLength(1);
      expect(orderBook.bids[0].amount).toBe(5);
    });

    it('should remove level when amount is 0', () => {
      updateOrderBook(orderBook, 'bids', { price: 100, amount: 1, total: 0 });
      expect(orderBook.bids).toHaveLength(1);
      updateOrderBook(orderBook, 'bids', { price: 100, amount: 0, total: 0 });
      expect(orderBook.bids).toHaveLength(0);
    });

    it('should not add level with 0 amount if it does not exist', () => {
      updateOrderBook(orderBook, 'bids', { price: 100, amount: 0, total: 0 });
      expect(orderBook.bids).toHaveLength(0);
    });
  });

  describe('sortOrderBook utility', () => {
    it('should sort bids descending by price', () => {
      orderBook.bids = [
        { price: 100, amount: 1, total: 0 },
        { price: 102, amount: 1, total: 0 },
        { price: 99, amount: 1, total: 0 },
      ];
      sortOrderBook(orderBook);
      expect(orderBook.bids.map((b) => b.price)).toEqual([102, 100, 99]);
    });

    it('should sort asks ascending by price', () => {
      orderBook.asks = [
        { price: 105, amount: 1, total: 0 },
        { price: 103, amount: 1, total: 0 },
        { price: 110, amount: 1, total: 0 },
      ];
      sortOrderBook(orderBook);
      expect(orderBook.asks.map((a) => a.price)).toEqual([103, 105, 110]);
    });
  });

  describe('calcOrderBookTotal utility', () => {
    it('should calculate cumulative totals for bids', () => {
      orderBook.bids = [
        { price: 102, amount: 1, total: 0 },
        { price: 100, amount: 2, total: 0 },
        { price: 99, amount: 3, total: 0 },
      ];
      calcOrderBookTotal(orderBook);
      expect(orderBook.bids[0].total).toBe(1);
      expect(orderBook.bids[1].total).toBe(3); // 1 + 2
      expect(orderBook.bids[2].total).toBe(6); // 1 + 2 + 3
    });

    it('should calculate cumulative totals for asks', () => {
      orderBook.asks = [
        { price: 103, amount: 1, total: 0 },
        { price: 105, amount: 2, total: 0 },
        { price: 110, amount: 3, total: 0 },
      ];
      calcOrderBookTotal(orderBook);
      expect(orderBook.asks[0].total).toBe(1);
      expect(orderBook.asks[1].total).toBe(3);
      expect(orderBook.asks[2].total).toBe(6);
    });
  });
});

describe('Backpack OrderBook Delta Updates', () => {
  /**
   * Simulates how Backpack WebSocket depth updates should be processed
   */
  it('should correctly apply delta updates after snapshot', () => {
    // Simulate initial snapshot from REST API
    const orderBook: OrderBook = {
      bids: [
        { price: 86310, amount: 0.5, total: 0 },
        { price: 86305, amount: 1.0, total: 0 },
        { price: 86300, amount: 2.0, total: 0 },
      ],
      asks: [
        { price: 86315, amount: 0.3, total: 0 },
        { price: 86320, amount: 0.8, total: 0 },
        { price: 86325, amount: 1.5, total: 0 },
      ],
    };

    sortOrderBook(orderBook);
    calcOrderBookTotal(orderBook);

    // Verify initial state
    expect(orderBook.bids[0].price).toBe(86310); // Highest bid
    expect(orderBook.asks[0].price).toBe(86315); // Lowest ask

    // Simulate WebSocket delta update: new bid at better price
    const deltaUpdate1 = {
      b: [['86312', '0.25']], // New best bid
      a: [],
    };

    deltaUpdate1.b.forEach(([price, amount]) => {
      updateOrderBook(orderBook, 'bids', {
        price: parseFloat(price),
        amount: parseFloat(amount),
        total: 0,
      });
    });

    sortOrderBook(orderBook);
    calcOrderBookTotal(orderBook);

    expect(orderBook.bids[0].price).toBe(86312); // New best bid
    expect(orderBook.bids[0].amount).toBe(0.25);

    // Simulate delta update: remove a bid level (amount = 0)
    const deltaUpdate2 = {
      b: [['86305', '0']], // Remove this level
      a: [],
    };

    deltaUpdate2.b.forEach(([price, amount]) => {
      updateOrderBook(orderBook, 'bids', {
        price: parseFloat(price),
        amount: parseFloat(amount),
        total: 0,
      });
    });

    sortOrderBook(orderBook);
    calcOrderBookTotal(orderBook);

    // 86305 should be gone
    expect(orderBook.bids.find((b) => b.price === 86305)).toBeUndefined();
    expect(orderBook.bids).toHaveLength(3); // 86312, 86310, 86300

    // Simulate delta update: update existing level
    const deltaUpdate3 = {
      b: [['86310', '0.75']], // Update amount
      a: [['86315', '0.5']], // Update ask too
    };

    deltaUpdate3.b.forEach(([price, amount]) => {
      updateOrderBook(orderBook, 'bids', {
        price: parseFloat(price),
        amount: parseFloat(amount),
        total: 0,
      });
    });

    deltaUpdate3.a.forEach(([price, amount]) => {
      updateOrderBook(orderBook, 'asks', {
        price: parseFloat(price),
        amount: parseFloat(amount),
        total: 0,
      });
    });

    sortOrderBook(orderBook);
    calcOrderBookTotal(orderBook);

    const bid86310 = orderBook.bids.find((b) => b.price === 86310);
    expect(bid86310?.amount).toBe(0.75);

    const ask86315 = orderBook.asks.find((a) => a.price === 86315);
    expect(ask86315?.amount).toBe(0.5);
  });

  it('should handle Backpack-style depth message format', () => {
    // This is the actual format from Backpack WebSocket
    const depthMessage = {
      data: {
        E: 1765825676701599,
        T: 1765825676678873,
        U: 2846129561,
        a: [] as [string, string][],
        b: [['86307.0', '0.09520']] as [string, string][],
        e: 'depth' as const,
        s: 'BTC_USDC_PERP',
        u: 2846129561,
      },
      stream: 'depth.BTC_USDC_PERP',
    };

    const orderBook: OrderBook = { bids: [], asks: [] };
    const eventData = depthMessage.data;

    // Process bid updates
    if (eventData.b && eventData.b.length > 0) {
      eventData.b.forEach((level) => {
        const [price, amount] = level;
        updateOrderBook(orderBook, 'bids', {
          price: parseFloat(price),
          amount: parseFloat(amount),
          total: 0,
        });
      });
    }

    // Process ask updates
    if (eventData.a && eventData.a.length > 0) {
      eventData.a.forEach((level) => {
        const [price, amount] = level;
        updateOrderBook(orderBook, 'asks', {
          price: parseFloat(price),
          amount: parseFloat(amount),
          total: 0,
        });
      });
    }

    sortOrderBook(orderBook);
    calcOrderBookTotal(orderBook);

    expect(orderBook.bids).toHaveLength(1);
    expect(orderBook.bids[0].price).toBe(86307);
    expect(orderBook.bids[0].amount).toBe(0.0952);
    expect(orderBook.asks).toHaveLength(0);
  });
});

describe('Backpack OrderBook Snapshot + Delta Workflow', () => {
  /**
   * Helper to apply a depth update to the orderbook (mirrors implementation)
   */
  const applyOrderBookUpdate = (orderBook: OrderBook, data: BackpackWsDepthUpdate) => {
    if (data.b && data.b.length > 0) {
      data.b.forEach(([price, amount]) => {
        updateOrderBook(orderBook, 'bids', {
          price: parseFloat(price),
          amount: parseFloat(amount),
          total: 0,
        });
      });
    }

    if (data.a && data.a.length > 0) {
      data.a.forEach(([price, amount]) => {
        updateOrderBook(orderBook, 'asks', {
          price: parseFloat(price),
          amount: parseFloat(amount),
          total: 0,
        });
      });
    }
  };

  it('should correctly process snapshot then delta updates', () => {
    // Simulate REST API snapshot response
    const snapshotResponse = {
      lastUpdateId: 1000,
      bids: [
        ['86310.0', '0.5'],
        ['86305.0', '1.0'],
        ['86300.0', '2.0'],
      ],
      asks: [
        ['86315.0', '0.3'],
        ['86320.0', '0.8'],
        ['86325.0', '1.5'],
      ],
    };

    // State tracking (similar to implementation)
    const state = {
      updates: [] as BackpackWsDepthUpdate[],
      isSnapshotLoaded: false,
      lastUpdateId: 0,
    };

    const orderBook: OrderBook = { bids: [], asks: [] };

    // Simulate WebSocket updates arriving BEFORE snapshot is loaded
    const earlyUpdate1: BackpackWsDepthUpdate = {
      e: 'depth',
      s: 'BTC_USDC_PERP',
      E: 1000001,
      b: [['86312.0', '0.25']],
      a: [],
    };

    const earlyUpdate2: BackpackWsDepthUpdate = {
      e: 'depth',
      s: 'BTC_USDC_PERP',
      E: 1000002,
      b: [],
      a: [['86313.0', '0.15']],
    };

    // Buffer early updates (snapshot not loaded)
    state.updates.push(earlyUpdate1);
    state.updates.push(earlyUpdate2);

    // Now load the snapshot
    orderBook.bids = snapshotResponse.bids.map(([price, amount]) => ({
      price: parseFloat(price),
      amount: parseFloat(amount),
      total: 0,
    }));

    orderBook.asks = snapshotResponse.asks.map(([price, amount]) => ({
      price: parseFloat(price),
      amount: parseFloat(amount),
      total: 0,
    }));

    state.lastUpdateId = snapshotResponse.lastUpdateId;

    // Apply buffered updates (in real impl, would filter by u > lastUpdateId)
    state.updates.forEach((update) => {
      applyOrderBookUpdate(orderBook, update);
    });

    sortOrderBook(orderBook);
    calcOrderBookTotal(orderBook);

    state.isSnapshotLoaded = true;
    state.updates = [];

    // Verify snapshot + buffered updates applied correctly
    expect(orderBook.bids[0].price).toBe(86312); // From early update
    expect(orderBook.bids[0].amount).toBe(0.25);
    expect(orderBook.asks[0].price).toBe(86313); // From early update
    expect(orderBook.asks[0].amount).toBe(0.15);

    // Now simulate a live update
    const liveUpdate: BackpackWsDepthUpdate = {
      e: 'depth',
      s: 'BTC_USDC_PERP',
      E: 1000003,
      b: [['86310.0', '0']], // Remove level
      a: [['86315.0', '0.5']], // Update level
    };

    applyOrderBookUpdate(orderBook, liveUpdate);
    sortOrderBook(orderBook);
    calcOrderBookTotal(orderBook);

    // Verify live update applied
    expect(orderBook.bids.find((b) => b.price === 86310)).toBeUndefined();
    const ask86315 = orderBook.asks.find((a) => a.price === 86315);
    expect(ask86315?.amount).toBe(0.5);
  });

  it('should handle multiple sequential updates correctly', () => {
    const orderBook: OrderBook = {
      bids: [
        { price: 100, amount: 1, total: 0 },
        { price: 99, amount: 2, total: 0 },
      ],
      asks: [
        { price: 101, amount: 1, total: 0 },
        { price: 102, amount: 2, total: 0 },
      ],
    };

    // Sequence of updates
    const updates: BackpackWsDepthUpdate[] = [
      { e: 'depth', s: 'TEST', E: 1, b: [['100.5', '0.5']], a: [] },
      { e: 'depth', s: 'TEST', E: 2, b: [['100', '0']], a: [] }, // Remove
      { e: 'depth', s: 'TEST', E: 3, b: [], a: [['100.8', '0.3']] },
      { e: 'depth', s: 'TEST', E: 4, b: [], a: [['101', '0']] }, // Remove
    ];

    updates.forEach((update) => {
      applyOrderBookUpdate(orderBook, update);
    });

    sortOrderBook(orderBook);
    calcOrderBookTotal(orderBook);

    // Verify final state
    expect(orderBook.bids).toHaveLength(2); // 100.5, 99
    expect(orderBook.bids[0].price).toBe(100.5);
    expect(orderBook.bids[1].price).toBe(99);

    expect(orderBook.asks).toHaveLength(2); // 100.8, 102
    expect(orderBook.asks[0].price).toBe(100.8);
    expect(orderBook.asks[1].price).toBe(102);
  });
});

describe('BackpackPublicWebSocket.listenOrderBook Integration', () => {
  let exchange: BackpackExchange;
  let store: DefaultStore;
  let publicWs: BackpackPublicWebSocket;
  let mockXhr: any;

  beforeEach(() => {
    store = new DefaultStore();
    store.update({
      markets: [
        {
          id: 'BTC_USDC_PERP',
          symbol: 'BTC_USDC_PERP',
          base: 'BTC',
          quote: 'USDC',
          active: true,
          precision: { amount: 5, price: 1 },
          limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 20 } },
        },
      ],
    });

    exchange = new BackpackExchange(
      'backpack',
      { key: 'test_key', secret: 'test_secret', testnet: false },
      store
    );

    // Mock xhr
    mockXhr = {
      get: vi.fn(),
    };
    exchange.xhr = mockXhr as any;

    publicWs = new BackpackPublicWebSocket(exchange);
  });

  it('should call callback with orderbook after snapshot loads', async () => {
    // Mock snapshot response
    mockXhr.get.mockResolvedValue({
      data: {
        lastUpdateId: 1000,
        bids: [
          ['86310.0', '0.5'],
          ['86305.0', '1.0'],
        ],
        asks: [
          ['86315.0', '0.3'],
          ['86320.0', '0.8'],
        ],
      },
    });

    // Track callback invocations
    const callbackResults: OrderBook[] = [];
    const callback = vi.fn((orderBook: OrderBook) => {
      // Deep copy to capture state at callback time
      callbackResults.push({
        bids: orderBook.bids.map((b) => ({ ...b })),
        asks: orderBook.asks.map((a) => ({ ...a })),
      });
    });

    // Simulate being connected by mocking the WebSocket
    (publicWs as any).ws = { readyState: WebSocket.OPEN };
    (publicWs as any).send = vi.fn();

    // Call listenOrderBook
    const unsubscribe = publicWs.listenOrderBook('BTC_USDC_PERP', callback);

    // Wait for async snapshot fetch
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Verify callback was called with snapshot data
    expect(callback).toHaveBeenCalled();
    expect(callbackResults.length).toBeGreaterThanOrEqual(1);

    const firstResult = callbackResults[0];
    expect(firstResult.bids).toHaveLength(2);
    expect(firstResult.bids[0].price).toBe(86310);
    expect(firstResult.bids[0].amount).toBe(0.5);
    expect(firstResult.asks).toHaveLength(2);
    expect(firstResult.asks[0].price).toBe(86315);
    expect(firstResult.asks[0].amount).toBe(0.3);

    // Verify totals are calculated
    expect(firstResult.bids[0].total).toBe(0.5);
    expect(firstResult.bids[1].total).toBe(1.5); // 0.5 + 1.0

    unsubscribe();
  });

  it('should call callback with updated orderbook after WebSocket delta', async () => {
    // Mock snapshot response
    mockXhr.get.mockResolvedValue({
      data: {
        lastUpdateId: 1000,
        bids: [['86310.0', '0.5']],
        asks: [['86315.0', '0.3']],
      },
    });

    const callbackResults: OrderBook[] = [];
    const callback = vi.fn((orderBook: OrderBook) => {
      callbackResults.push({
        bids: orderBook.bids.map((b) => ({ ...b })),
        asks: orderBook.asks.map((a) => ({ ...a })),
      });
    });

    (publicWs as any).ws = { readyState: WebSocket.OPEN };
    (publicWs as any).send = vi.fn();

    publicWs.listenOrderBook('BTC_USDC_PERP', callback);

    // Wait for snapshot
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Get the message handler that was registered
    const handlers = (publicWs as any).messageHandlers['depth.BTC_USDC_PERP'];
    expect(handlers).toBeDefined();
    expect(handlers.length).toBeGreaterThan(0);

    // Simulate a WebSocket delta update
    const deltaUpdate: BackpackWsDepthUpdate = {
      e: 'depth',
      s: 'BTC_USDC_PERP',
      E: Date.now() * 1000,
      b: [['86312.0', '0.25']], // New best bid
      a: [],
      u: 1001,
    };

    // Call the handler directly (simulating onMessage)
    handlers[0](deltaUpdate);

    // Verify callback was called again
    expect(callbackResults.length).toBe(2);

    const updatedResult = callbackResults[1];
    expect(updatedResult.bids).toHaveLength(2);
    expect(updatedResult.bids[0].price).toBe(86312); // New best bid
    expect(updatedResult.bids[0].amount).toBe(0.25);
  });

  it('should buffer updates received before snapshot loads', async () => {
    // Delay the snapshot response
    mockXhr.get.mockImplementation(
      () =>
        new Promise((resolve) =>
          setTimeout(
            () =>
              resolve({
                data: {
                  lastUpdateId: 1000,
                  bids: [['86310.0', '0.5']],
                  asks: [['86315.0', '0.3']],
                },
              }),
            100
          )
        )
    );

    const callbackResults: OrderBook[] = [];
    const callback = vi.fn((orderBook: OrderBook) => {
      callbackResults.push({
        bids: orderBook.bids.map((b) => ({ ...b })),
        asks: orderBook.asks.map((a) => ({ ...a })),
      });
    });

    (publicWs as any).ws = { readyState: WebSocket.OPEN };
    (publicWs as any).send = vi.fn();

    publicWs.listenOrderBook('BTC_USDC_PERP', callback);

    // Wait a bit for subscription but not for snapshot
    await new Promise((resolve) => setTimeout(resolve, 20));

    // Get handler and simulate early updates (before snapshot)
    const handlers = (publicWs as any).messageHandlers['depth.BTC_USDC_PERP'];

    // Send update while snapshot is loading (should be buffered)
    const earlyUpdate: BackpackWsDepthUpdate = {
      e: 'depth',
      s: 'BTC_USDC_PERP',
      E: Date.now() * 1000,
      b: [['86312.0', '0.25']],
      a: [],
      u: 1001,
    };
    handlers[0](earlyUpdate);

    // Callback should NOT have been called yet (snapshot not loaded)
    expect(callback).not.toHaveBeenCalled();

    // Wait for snapshot to load
    await new Promise((resolve) => setTimeout(resolve, 150));

    // Now callback should have been called with snapshot + buffered update
    expect(callback).toHaveBeenCalled();
    expect(callbackResults.length).toBeGreaterThanOrEqual(1);

    const result = callbackResults[0];
    // Should have the buffered update applied (86312 is better than 86310)
    expect(result.bids[0].price).toBe(86312);
  });

  it('should refetch snapshot on WebSocket reconnect', async () => {
    // First snapshot
    mockXhr.get.mockResolvedValueOnce({
      data: {
        lastUpdateId: 1000,
        bids: [['86310.0', '0.5']],
        asks: [['86315.0', '0.3']],
      },
    });

    const callbackResults: OrderBook[] = [];
    const callback = vi.fn((orderBook: OrderBook) => {
      callbackResults.push({
        bids: orderBook.bids.map((b) => ({ ...b })),
        asks: orderBook.asks.map((a) => ({ ...a })),
      });
    });

    (publicWs as any).ws = { readyState: WebSocket.OPEN };
    (publicWs as any).send = vi.fn();

    publicWs.listenOrderBook('BTC_USDC_PERP', callback);

    // Wait for initial snapshot
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(callback).toHaveBeenCalledTimes(1);
    expect(callbackResults[0].bids[0].price).toBe(86310);

    // Simulate reconnection - new snapshot with different data
    mockXhr.get.mockResolvedValueOnce({
      data: {
        lastUpdateId: 2000,
        bids: [['86400.0', '1.0']], // Price changed
        asks: [['86410.0', '0.8']],
      },
    });

    // Call onOpen to simulate reconnection
    (publicWs as any).onOpen();

    // Wait for new snapshot
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Should have called callback again with new snapshot
    expect(callback).toHaveBeenCalledTimes(2);
    expect(callbackResults[1].bids[0].price).toBe(86400); // New price from reconnect snapshot
    expect(callbackResults[1].asks[0].price).toBe(86410);
  });

  it('should reset orderbook state on reconnect to buffer new updates', async () => {
    // First snapshot
    mockXhr.get.mockResolvedValueOnce({
      data: {
        lastUpdateId: 1000,
        bids: [['86310.0', '0.5']],
        asks: [['86315.0', '0.3']],
      },
    });

    const callbackResults: OrderBook[] = [];
    const callback = vi.fn((orderBook: OrderBook) => {
      callbackResults.push({
        bids: orderBook.bids.map((b) => ({ ...b })),
        asks: orderBook.asks.map((a) => ({ ...a })),
      });
    });

    (publicWs as any).ws = { readyState: WebSocket.OPEN };
    (publicWs as any).send = vi.fn();

    publicWs.listenOrderBook('BTC_USDC_PERP', callback);

    // Wait for initial snapshot
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(callback).toHaveBeenCalledTimes(1);

    // Verify state is marked as loaded
    let state = (publicWs as any).orderBookState['BTC_USDC_PERP'];
    expect(state.isSnapshotLoaded).toBe(true);

    // Simulate reconnection - make snapshot take longer
    mockXhr.get.mockImplementationOnce(
      () =>
        new Promise((resolve) =>
          setTimeout(
            () =>
              resolve({
                data: {
                  lastUpdateId: 2000,
                  bids: [['86400.0', '1.0']],
                  asks: [['86410.0', '0.8']],
                },
              }),
            100
          )
        )
    );

    // Call onOpen to simulate reconnection
    (publicWs as any).onOpen();

    // Re-read state after reconnect (resetAndRefetchOrderBooks creates new object)
    state = (publicWs as any).orderBookState['BTC_USDC_PERP'];

    // Immediately after reconnect, state should be reset
    expect(state.isSnapshotLoaded).toBe(false);
    expect(state.updates).toHaveLength(0);

    // Send update while reconnect snapshot is loading (should be buffered)
    const handlers = (publicWs as any).messageHandlers['depth.BTC_USDC_PERP'];
    const updateDuringReconnect: BackpackWsDepthUpdate = {
      e: 'depth',
      s: 'BTC_USDC_PERP',
      E: Date.now() * 1000,
      b: [['86405.0', '0.1']],
      a: [],
      u: 2001,
    };
    handlers[0](updateDuringReconnect);

    // Should be buffered
    expect(state.updates).toHaveLength(1);

    // Wait for new snapshot
    await new Promise((resolve) => setTimeout(resolve, 150));

    // Should have new data with buffered update applied
    expect(callback).toHaveBeenCalledTimes(2);
    expect(callbackResults[1].bids[0].price).toBe(86405); // From buffered update
  });
});
