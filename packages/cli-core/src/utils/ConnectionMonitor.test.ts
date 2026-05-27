import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ExchangeLike, LastReceivedMessage } from '../types';

import {
  ConnectionMonitor,
  formatConnectionStatusLine,
  getConnectionStatus,
  type ConnectionHealth,
  type ConnectionStatus,
} from './ConnectionMonitor';

// Helper to create mock exchange
function createMockExchange(overrides: Partial<ExchangeLike> = {}): ExchangeLike {
  return {
    name: 'test-exchange',
    emitter: {
      on: vi.fn(),
      off: vi.fn(),
      emit: vi.fn(),
    },
    ...overrides,
  };
}

describe('ConnectionMonitor', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('constructor', () => {
    it('should initialize with exchange and callbacks', () => {
      const exchange = createMockExchange();
      const callbacks = { onWarning: vi.fn() };

      const monitor = new ConnectionMonitor(exchange, callbacks);

      expect(monitor).toBeInstanceOf(ConnectionMonitor);
    });

    it('should initialize with default empty callbacks', () => {
      const exchange = createMockExchange();

      const monitor = new ConnectionMonitor(exchange);

      expect(monitor).toBeInstanceOf(ConnectionMonitor);
    });
  });

  describe('start', () => {
    it('should start interval monitoring', () => {
      const exchange = createMockExchange();
      const monitor = new ConnectionMonitor(exchange);

      monitor.start();

      expect(exchange.emitter.on).toHaveBeenCalledWith('ws-reconnected', expect.any(Function));
    });

    it('should not start twice if already started', () => {
      const exchange = createMockExchange();
      const monitor = new ConnectionMonitor(exchange);

      monitor.start();
      monitor.start();

      expect(exchange.emitter.on).toHaveBeenCalledTimes(1);
    });

    it('should use custom interval', () => {
      const exchange = createMockExchange();
      const onTick = vi.fn();
      const monitor = new ConnectionMonitor(exchange, { onTick });

      monitor.start(1000);

      // Advance by custom interval
      vi.advanceTimersByTime(1000);
      expect(onTick).toHaveBeenCalledTimes(1);

      vi.advanceTimersByTime(1000);
      expect(onTick).toHaveBeenCalledTimes(2);

      monitor.stop();
    });
  });

  describe('stop', () => {
    it('should stop interval monitoring', () => {
      const exchange = createMockExchange();
      const onTick = vi.fn();
      const monitor = new ConnectionMonitor(exchange, { onTick });

      monitor.start(1000);
      vi.advanceTimersByTime(1000);
      expect(onTick).toHaveBeenCalledTimes(1);

      monitor.stop();
      vi.advanceTimersByTime(5000);
      expect(onTick).toHaveBeenCalledTimes(1); // No more calls after stop
    });

    it('should remove reconnect listener', () => {
      const exchange = createMockExchange();
      const monitor = new ConnectionMonitor(exchange);

      monitor.start();
      monitor.stop();

      expect(exchange.emitter.off).toHaveBeenCalledWith('ws-reconnected', expect.any(Function));
    });

    it('should handle stop when not started', () => {
      const exchange = createMockExchange();
      const monitor = new ConnectionMonitor(exchange);

      // Should not throw
      monitor.stop();

      expect(exchange.emitter.off).toHaveBeenCalled();
    });
  });

  describe('getStatus', () => {
    it('should return initializing status when no data available', () => {
      const exchange = createMockExchange();
      const monitor = new ConnectionMonitor(exchange);

      const status = monitor.getStatus();

      expect(status.overall).toBe('initializing');
      expect(status.public.status).toBe('initializing');
      expect(status.private.status).toBe('initializing');
    });

    it('should return healthy status when recent data received', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 1000, // 1 second ago
            private: now - 1000,
          },
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      const status = monitor.getStatus();

      expect(status.overall).toBe('healthy');
      expect(status.public.status).toBe('healthy');
      expect(status.private.status).toBe('healthy');
    });

    it('should return warning status when data is stale (10-30 seconds)', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 15000, // 15 seconds ago
            private: now - 1000,
          },
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      const status = monitor.getStatus();

      expect(status.overall).toBe('warning');
      expect(status.public.status).toBe('warning');
      expect(status.private.status).toBe('healthy');
    });

    it('should return error status when data is very stale (>30 seconds)', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 35000, // 35 seconds ago
            private: now - 1000,
          },
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      const status = monitor.getStatus();

      expect(status.overall).toBe('error');
      expect(status.public.status).toBe('error');
      expect(status.private.status).toBe('healthy');
    });

    it('should use WebSocket timestamps when store timestamps unavailable', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        publicWebsocket: { lastReceived: now - 1000 },
        privateWebsocket: { lastReceived: now - 1000 },
      });
      const monitor = new ConnectionMonitor(exchange);

      const status = monitor.getStatus();

      expect(status.overall).toBe('healthy');
    });

    it('should include latency from store', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 1000,
            private: now - 1000,
          },
          latency: 50,
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      const status = monitor.getStatus();

      expect(status.latency).toBe(50);
    });

    it('should include futures status when available', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 1000,
            private: now - 1000,
            futures: now - 1000,
          },
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      const status = monitor.getStatus();

      expect(status.futures).toBeDefined();
      expect(status.futures?.status).toBe('healthy');
    });

    it('should include business status when available', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 1000,
            private: now - 1000,
            business: now - 1000,
          },
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      const status = monitor.getStatus();

      expect(status.business).toBeDefined();
      expect(status.business?.status).toBe('healthy');
    });

    it('should return lastMessageAgo for each stream', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 5000,
            private: now - 3000,
          },
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      const status = monitor.getStatus();

      expect(status.public.lastMessageAgo).toBeGreaterThanOrEqual(5000);
      expect(status.private.lastMessageAgo).toBeGreaterThanOrEqual(3000);
    });

    it('should return null lastMessageAgo when no timestamp', () => {
      const exchange = createMockExchange();
      const monitor = new ConnectionMonitor(exchange);

      const status = monitor.getStatus();

      expect(status.public.lastMessageAgo).toBeNull();
      expect(status.private.lastMessageAgo).toBeNull();
    });
  });

  describe('getPromptStatus', () => {
    it('should return empty string for healthy status', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 1000,
            private: now - 1000,
          },
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      expect(monitor.getPromptStatus()).toBe('');
    });

    it('should return [SLOW] for warning status', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 15000,
            private: now - 15000,
          },
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      expect(monitor.getPromptStatus()).toBe('[SLOW] ');
    });

    it('should return [!CONN] for error status', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 35000,
            private: now - 35000,
          },
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      expect(monitor.getPromptStatus()).toBe('[!CONN] ');
    });

    it('should return [...] for initializing status', () => {
      const exchange = createMockExchange();
      const monitor = new ConnectionMonitor(exchange);

      expect(monitor.getPromptStatus()).toBe('[...] ');
    });
  });

  describe('callbacks', () => {
    it('should call onTick on each interval', () => {
      const exchange = createMockExchange();
      const onTick = vi.fn();
      const monitor = new ConnectionMonitor(exchange, { onTick });

      monitor.start(1000);

      vi.advanceTimersByTime(1000);
      expect(onTick).toHaveBeenCalledTimes(1);

      vi.advanceTimersByTime(1000);
      expect(onTick).toHaveBeenCalledTimes(2);

      monitor.stop();
    });

    it('should call onWarning when transitioning to warning', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 15000,
            private: now - 15000,
          },
        },
      });
      const onWarning = vi.fn();
      const monitor = new ConnectionMonitor(exchange, { onWarning });

      monitor.start(1000);

      // Need to advance past init grace period (60 seconds)
      vi.advanceTimersByTime(61000);

      expect(onWarning).toHaveBeenCalledWith(expect.stringContaining('Connection slow'));

      monitor.stop();
    });

    it('should call onError when transitioning to error', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 35000,
            private: now - 35000,
          },
        },
      });
      const onError = vi.fn();
      const monitor = new ConnectionMonitor(exchange, { onError });

      monitor.start(1000);

      // Advance past init grace period
      vi.advanceTimersByTime(61000);

      expect(onError).toHaveBeenCalledWith(expect.stringContaining('Connection issues'));

      monitor.stop();
    });

    it('should call onRecovered when transitioning from error to healthy', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      // Start with error state
      const lastReceivedMessage: LastReceivedMessage = {
        public: now - 35000,
        private: now - 35000,
      };

      const exchange = createMockExchange({
        store: { lastReceivedMessage },
      });
      const onRecovered = vi.fn();
      const onError = vi.fn();
      const monitor = new ConnectionMonitor(exchange, { onRecovered, onError });

      monitor.start(1000);

      // Advance past init grace period to trigger error
      vi.advanceTimersByTime(61000);
      expect(onError).toHaveBeenCalled();

      // Update to healthy state
      lastReceivedMessage.public = Date.now() - 1000;
      lastReceivedMessage.private = Date.now() - 1000;

      vi.advanceTimersByTime(1000);

      expect(onRecovered).toHaveBeenCalledWith('Connection recovered');

      monitor.stop();
    });

    it('should call onRecovered when ws-reconnected event fired', () => {
      const exchange = createMockExchange();
      const onRecovered = vi.fn();
      const monitor = new ConnectionMonitor(exchange, { onRecovered });

      monitor.start();

      // Get the handler and call it
      const onCall = (exchange.emitter.on as ReturnType<typeof vi.fn>).mock.calls.find(
        (call) => call[0] === 'ws-reconnected',
      );
      const handler = onCall?.[1];
      handler?.();

      expect(onRecovered).toHaveBeenCalledWith('WebSocket reconnected');

      monitor.stop();
    });

    it('should not call onWarning twice for same warning', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 15000,
            private: now - 15000,
          },
        },
      });
      const onWarning = vi.fn();
      const monitor = new ConnectionMonitor(exchange, { onWarning });

      monitor.start(1000);

      // Advance past init grace period
      vi.advanceTimersByTime(61000);
      expect(onWarning).toHaveBeenCalledTimes(1);

      // Continue in warning state
      vi.advanceTimersByTime(5000);
      expect(onWarning).toHaveBeenCalledTimes(1); // Still 1

      monitor.stop();
    });

    it('should not alert during init grace period', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      const exchange = createMockExchange(); // No data = initializing
      const onWarning = vi.fn();
      const onError = vi.fn();
      const monitor = new ConnectionMonitor(exchange, { onWarning, onError });

      monitor.start(1000);

      // Stay within grace period
      vi.advanceTimersByTime(30000);

      expect(onWarning).not.toHaveBeenCalled();
      expect(onError).not.toHaveBeenCalled();

      monitor.stop();
    });
  });

  describe('status combination', () => {
    it('should return error if either public or private is error', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 35000, // error
            private: now - 1000, // healthy
          },
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      expect(monitor.getStatus().overall).toBe('error');
    });

    it('should return warning if one is warning and other is healthy', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 15000, // warning
            private: now - 1000, // healthy
          },
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      expect(monitor.getStatus().overall).toBe('warning');
    });

    it('should return initializing if one is initializing and other is healthy', () => {
      const now = Date.now();
      const exchange = createMockExchange({
        store: {
          lastReceivedMessage: {
            public: now - 1000, // healthy
            // private: undefined - initializing
          },
        },
      });
      const monitor = new ConnectionMonitor(exchange);

      expect(monitor.getStatus().overall).toBe('initializing');
    });
  });
});

describe('getConnectionStatus', () => {
  it('should return one-shot status', () => {
    const now = Date.now();
    const exchange = createMockExchange({
      store: {
        lastReceivedMessage: {
          public: now - 1000,
          private: now - 1000,
        },
      },
    });

    const status = getConnectionStatus(exchange);

    expect(status.overall).toBe('healthy');
  });
});

describe('formatConnectionStatusLine', () => {
  it('should format healthy status', () => {
    const status: ConnectionStatus = {
      overall: 'healthy',
      public: { status: 'healthy', lastMessageAgo: 1000 },
      private: { status: 'healthy', lastMessageAgo: 1000 },
    };

    const line = formatConnectionStatusLine(status);

    expect(line).toBe('Public:OK | Private:OK');
  });

  it('should format status with latency', () => {
    const status: ConnectionStatus = {
      overall: 'healthy',
      public: { status: 'healthy', lastMessageAgo: 1000 },
      private: { status: 'healthy', lastMessageAgo: 1000 },
      latency: 50,
    };

    const line = formatConnectionStatusLine(status);

    expect(line).toBe('Public:OK | Private:OK | 50ms');
  });

  it('should format warning status', () => {
    const status: ConnectionStatus = {
      overall: 'warning',
      public: { status: 'warning', lastMessageAgo: 15000 },
      private: { status: 'healthy', lastMessageAgo: 1000 },
    };

    const line = formatConnectionStatusLine(status);

    expect(line).toBe('Public:SLOW | Private:OK');
  });

  it('should format error status', () => {
    const status: ConnectionStatus = {
      overall: 'error',
      public: { status: 'error', lastMessageAgo: 35000 },
      private: { status: 'error', lastMessageAgo: 35000 },
    };

    const line = formatConnectionStatusLine(status);

    expect(line).toBe('Public:ERR | Private:ERR');
  });

  it('should format initializing status', () => {
    const status: ConnectionStatus = {
      overall: 'initializing',
      public: { status: 'initializing', lastMessageAgo: null },
      private: { status: 'initializing', lastMessageAgo: null },
    };

    const line = formatConnectionStatusLine(status);

    expect(line).toBe('Public:... | Private:...');
  });

  it('should format disconnected status', () => {
    const status: ConnectionStatus = {
      overall: 'disconnected',
      public: { status: 'disconnected', lastMessageAgo: null },
      private: { status: 'disconnected', lastMessageAgo: null },
    };

    const line = formatConnectionStatusLine(status);

    expect(line).toBe('Public:X | Private:X');
  });
});
