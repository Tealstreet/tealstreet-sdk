import type { Exchange } from '../../exchanges/base';
import type { BaseWebSocket } from '../../exchanges/base.ws';
import type { MessageHandlersStats } from '../../exchanges/types';
import type { ExchangeName, ExchangeOptions, StoreData, Writable } from '../../types';

import { beforeEach, describe, expect, it } from 'vitest';

import { MessageHandlerStatsPostprocessor } from './MessageHandlerStatsPostprocessor';

// Helper to create a mock websocket with configurable stats and initialized state
function createMockWebSocket(stats: MessageHandlersStats = {}, isInitialized = true): BaseWebSocket<any> {
  return {
    isInitialized,
    messageHandlersStats: stats,
  } as unknown as BaseWebSocket<any>;
}

// Helper to create a mock exchange
function createMockExchange(overrides: Partial<Exchange> = {}): Exchange {
  return {
    publicWebsocket: undefined,
    publicWebsocketSecondary: undefined,
    publicInverseWebsocket: undefined,
    privateWebsocket: undefined,
    businessWebsocket: undefined,
    futuresWebsocket: undefined,
    ...overrides,
  } as unknown as Exchange;
}

// Helper to create a minimal writable store
function createStore(overrides: Partial<StoreData> = {}): Writable<StoreData> {
  return {
    ...overrides,
  } as Writable<StoreData>;
}

describe('MessageHandlerStatsPostprocessor', () => {
  let processor: MessageHandlerStatsPostprocessor;

  describe('processes stats from initialized websockets', () => {
    it('should collect stats from an initialized public websocket', () => {
      const publicStats: MessageHandlersStats = {
        'orderbook.BTCUSDT': { lastMessageTimestamp: 1000 },
      };
      const exchange = createMockExchange({
        publicWebsocket: createMockWebSocket(publicStats, true),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bybit' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      expect(store.messageHandlerStats).toBeDefined();
      expect(store.messageHandlerStats!.public).toEqual(publicStats);
    });

    it('should collect stats from an initialized private websocket', () => {
      const privateStats: MessageHandlersStats = {
        'order.update': { lastMessageTimestamp: 2000 },
      };
      const exchange = createMockExchange({
        privateWebsocket: createMockWebSocket(privateStats, true),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'binance' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      expect(store.messageHandlerStats!.private).toEqual(privateStats);
    });

    it('should collect stats from business and futures websockets', () => {
      const businessStats: MessageHandlersStats = {
        'wallet.update': { lastMessageTimestamp: 3000 },
      };
      const futuresStats: MessageHandlersStats = {
        'position.update': { lastMessageTimestamp: 4000 },
      };
      const exchange = createMockExchange({
        businessWebsocket: createMockWebSocket(businessStats, true),
        futuresWebsocket: createMockWebSocket(futuresStats, true),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bybit' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      expect(store.messageHandlerStats!.business).toEqual(businessStats);
      expect(store.messageHandlerStats!.futures).toEqual(futuresStats);
    });

    it('should collect stats from all four websocket types simultaneously', () => {
      const exchange = createMockExchange({
        publicWebsocket: createMockWebSocket({ pubTopic: { lastMessageTimestamp: 1 } }, true),
        privateWebsocket: createMockWebSocket({ privTopic: { lastMessageTimestamp: 2 } }, true),
        businessWebsocket: createMockWebSocket({ bizTopic: { lastMessageTimestamp: 3 } }, true),
        futuresWebsocket: createMockWebSocket({ futTopic: { lastMessageTimestamp: 4 } }, true),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bybit' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      expect(store.messageHandlerStats!.public).toBeDefined();
      expect(store.messageHandlerStats!.private).toBeDefined();
      expect(store.messageHandlerStats!.business).toBeDefined();
      expect(store.messageHandlerStats!.futures).toBeDefined();
    });
  });

  describe('skips dead/noop websockets', () => {
    it('should skip websocket that is not initialized AND has empty stats', () => {
      const exchange = createMockExchange({
        publicWebsocket: createMockWebSocket({}, false),
        privateWebsocket: createMockWebSocket({ topic: { lastMessageTimestamp: 1 } }, true),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bybit' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      // Public should not be set (dead websocket)
      expect(store.messageHandlerStats!.public).toBeUndefined();
      // Private should still be set
      expect(store.messageHandlerStats!.private).toBeDefined();
    });

    it('should include websocket that is not initialized but has non-empty stats', () => {
      // Edge case: websocket has stats from before it was marked uninitialized
      const stats: MessageHandlersStats = {
        topic: { lastMessageTimestamp: 500 },
      };
      const exchange = createMockExchange({
        publicWebsocket: createMockWebSocket(stats, false),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bybit' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      // Should be included because stats are non-empty
      expect(store.messageHandlerStats!.public).toEqual(stats);
    });

    it('should include websocket that is initialized but has empty stats', () => {
      const exchange = createMockExchange({
        publicWebsocket: createMockWebSocket({}, true),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bybit' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      // Should be included because it's initialized
      expect(store.messageHandlerStats!.public).toEqual({});
    });

    it('should skip undefined websockets entirely', () => {
      const exchange = createMockExchange({
        // All websockets undefined (default)
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bybit' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      expect(store.messageHandlerStats).toEqual({});
    });
  });

  describe('publicInverseWebsocket mapping', () => {
    it('should map publicInverseWebsocket stats to the public key', () => {
      const inverseStats: MessageHandlersStats = {
        'orderbook.inverse': { lastMessageTimestamp: 7000 },
      };
      const exchange = createMockExchange({
        publicInverseWebsocket: createMockWebSocket(inverseStats, true),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bybitinverse' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      expect(store.messageHandlerStats!.public).toEqual(inverseStats);
    });

    it('should overwrite regular public stats when publicInverseWebsocket is present', () => {
      const regularPublicStats: MessageHandlersStats = {
        'orderbook.regular': { lastMessageTimestamp: 1000 },
      };
      const inverseStats: MessageHandlersStats = {
        'orderbook.inverse': { lastMessageTimestamp: 7000 },
      };
      const exchange = createMockExchange({
        publicWebsocket: createMockWebSocket(regularPublicStats, true),
        publicInverseWebsocket: createMockWebSocket(inverseStats, true),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bybitinverse' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      // publicInverseWebsocket overwrites the public stats
      expect(store.messageHandlerStats!.public).toEqual(inverseStats);
    });
  });

  describe('publicWebsocketSecondary merging', () => {
    it('should merge secondary public websocket stats into public key', () => {
      const primaryStats: MessageHandlersStats = {
        'orderbook.BTCUSDT': { lastMessageTimestamp: 1000 },
      };
      const secondaryStats: MessageHandlersStats = {
        'ticker.BTCUSDT': { lastMessageTimestamp: 2000 },
      };
      const exchange = createMockExchange({
        publicWebsocket: createMockWebSocket(primaryStats, true),
        publicWebsocketSecondary: createMockWebSocket(secondaryStats, true),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bitunix' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      // Both primary and secondary stats should be merged
      expect(store.messageHandlerStats!.public).toEqual({
        'ticker.BTCUSDT': { lastMessageTimestamp: 2000 },
        'orderbook.BTCUSDT': { lastMessageTimestamp: 1000 },
      });
    });

    it('should give primary stats priority over secondary when keys overlap', () => {
      const primaryStats: MessageHandlersStats = {
        sharedTopic: { lastMessageTimestamp: 5000 },
      };
      const secondaryStats: MessageHandlersStats = {
        sharedTopic: { lastMessageTimestamp: 1000 },
      };
      const exchange = createMockExchange({
        publicWebsocket: createMockWebSocket(primaryStats, true),
        publicWebsocketSecondary: createMockWebSocket(secondaryStats, true),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bitunix' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      // Primary (spread last) should win
      expect(store.messageHandlerStats!.public!.sharedTopic).toEqual({
        lastMessageTimestamp: 5000,
      });
    });

    it('should not merge secondary if public stats do not exist yet', () => {
      // publicWebsocket is a dead noop (not initialized, empty stats)
      // publicWebsocketSecondary exists but public key was never set
      const secondaryStats: MessageHandlersStats = {
        'ticker.BTCUSDT': { lastMessageTimestamp: 2000 },
      };
      const exchange = createMockExchange({
        publicWebsocket: createMockWebSocket({}, false), // dead — skipped
        publicWebsocketSecondary: createMockWebSocket(secondaryStats, true),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bitunix' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      // public was never set (dead ws), so secondary merge condition fails
      // The code checks `store.messageHandlerStats.public` before merging
      expect(store.messageHandlerStats!.public).toBeUndefined();
    });
  });

  describe('does NOT overwrite stats with empty data from uninitialized websockets', () => {
    it('should not overwrite existing public stats with empty uninitialized websocket', () => {
      // First, simulate a websocket that was initialized and produced stats
      const goodStats: MessageHandlersStats = {
        topic1: { lastMessageTimestamp: 9000 },
      };
      const exchange = createMockExchange({
        publicWebsocket: createMockWebSocket(goodStats, true),
        // A dead private websocket should not affect public stats
        privateWebsocket: createMockWebSocket({}, false),
      });
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bybit' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      processor.process(store);

      expect(store.messageHandlerStats!.public).toEqual(goodStats);
      // Dead private websocket should not create an entry
      expect(store.messageHandlerStats!.private).toBeUndefined();
    });

    it('should initialize messageHandlerStats if not present on store', () => {
      const exchange = createMockExchange();
      processor = new MessageHandlerStatsPostprocessor(
        'account',
        'bybit' as ExchangeName,
        {} as ExchangeOptions,
        exchange,
      );
      const store = createStore();

      // Ensure messageHandlerStats is not set
      expect(store.messageHandlerStats).toBeUndefined();

      processor.process(store);

      // Should have been initialized to {}
      expect(store.messageHandlerStats).toBeDefined();
      expect(store.messageHandlerStats).toEqual({});
    });
  });
});
