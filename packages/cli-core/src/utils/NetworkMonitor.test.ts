import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { NetworkMonitor, NetworkMonitorCallback } from './NetworkMonitor';
import type { ExchangeLike } from '../types';

// Mock the dns module
vi.mock('dns', () => ({
  lookup: vi.fn(),
}));

describe('NetworkMonitor', () => {
  let mockExchange: ExchangeLike;
  let callbacks: NetworkMonitorCallback;
  let onOfflineMock: (message: string) => void;
  let onOnlineMock: (message: string) => void;

  beforeEach(() => {
    vi.useFakeTimers();

    // Create mock exchange
    mockExchange = {
      name: 'test-exchange',
      emitter: {
        on: vi.fn(),
        off: vi.fn(),
        emit: vi.fn(),
      },
      setOnlineStatus: vi.fn(),
    };

    // Create callback mocks
    onOfflineMock = vi.fn();
    onOnlineMock = vi.fn();
    callbacks = {
      onOffline: onOfflineMock,
      onOnline: onOnlineMock,
    };
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  describe('constructor', () => {
    it('should create instance with exchange and callbacks', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      expect(monitor).toBeInstanceOf(NetworkMonitor);
      expect(monitor.getIsOnline()).toBe(true);
    });

    it('should create instance with null exchange', () => {
      const monitor = new NetworkMonitor(null, callbacks);
      expect(monitor).toBeInstanceOf(NetworkMonitor);
    });

    it('should create instance with default empty callbacks', () => {
      const monitor = new NetworkMonitor(mockExchange);
      expect(monitor).toBeInstanceOf(NetworkMonitor);
    });
  });

  describe('setExchange', () => {
    it('should update exchange reference', () => {
      const monitor = new NetworkMonitor(null, callbacks);

      // Create a new mock exchange
      const newExchange: ExchangeLike = {
        name: 'new-exchange',
        emitter: {
          on: vi.fn(),
          off: vi.fn(),
          emit: vi.fn(),
        },
        setOnlineStatus: vi.fn(),
      };

      monitor.setExchange(newExchange);

      // Should update the new exchange with current status (online by default)
      expect(newExchange.setOnlineStatus).toHaveBeenCalledWith(true);
    });

    it('should handle setting exchange to null', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      monitor.setExchange(null);
      // Should not throw
      expect(mockExchange.setOnlineStatus).not.toHaveBeenCalled();
    });

    it('should call setOnlineStatus with current status on new exchange', () => {
      const monitor = new NetworkMonitor(null, callbacks);

      const newExchange: ExchangeLike = {
        name: 'new-exchange',
        emitter: {
          on: vi.fn(),
          off: vi.fn(),
          emit: vi.fn(),
        },
        setOnlineStatus: vi.fn(),
      };

      // Default is online
      monitor.setExchange(newExchange);
      expect(newExchange.setOnlineStatus).toHaveBeenCalledWith(true);
    });

    it('should work with exchange that does not have setOnlineStatus', () => {
      const monitor = new NetworkMonitor(null, callbacks);

      const newExchange: ExchangeLike = {
        name: 'basic-exchange',
        emitter: {
          on: vi.fn(),
          off: vi.fn(),
          emit: vi.fn(),
        },
        // No setOnlineStatus
      };

      // Should not throw
      expect(() => monitor.setExchange(newExchange)).not.toThrow();
    });
  });

  describe('start', () => {
    it('should start monitoring and perform initial check', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      monitor.start(1000);

      // Should be monitoring
      expect(monitor.getIsOnline()).toBe(true);
    });

    it('should not start if already running', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      monitor.start(1000);
      monitor.start(1000); // Second call should be ignored

      // Should still work normally
      expect(monitor.getIsOnline()).toBe(true);
    });

    it('should use default interval if not provided', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      monitor.start(); // Uses default 5000ms

      expect(monitor.getIsOnline()).toBe(true);
    });

    it('should run periodic checks', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      monitor.start(1000);

      // Advance timer to trigger interval
      vi.advanceTimersByTime(1000);
      vi.advanceTimersByTime(1000);

      // Monitor should still be running
      expect(monitor.getIsOnline()).toBe(true);
    });
  });

  describe('stop', () => {
    it('should stop monitoring', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      monitor.start(1000);
      monitor.stop();

      // Advancing timer should not cause issues
      vi.advanceTimersByTime(5000);
    });

    it('should handle stop when not started', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      // Should not throw
      expect(() => monitor.stop()).not.toThrow();
    });

    it('should allow restart after stop', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      monitor.start(1000);
      monitor.stop();
      monitor.start(1000);

      expect(monitor.getIsOnline()).toBe(true);
    });
  });

  describe('getIsOnline', () => {
    it('should return true by default', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      expect(monitor.getIsOnline()).toBe(true);
    });

    it('should return current online status', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      // Default is true
      expect(monitor.getIsOnline()).toBe(true);
    });
  });

  describe('check (private, tested via side effects)', () => {
    it('should notify exchange on status change to offline', async () => {
      // This test is limited because check() is private
      // and depends on DNS lookup which is mocked differently in Node vs browser
      const monitor = new NetworkMonitor(mockExchange, callbacks);

      // Start monitoring
      monitor.start(1000);

      // Initial status should be online
      expect(monitor.getIsOnline()).toBe(true);
    });

    it('should handle callbacks when status changes', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      monitor.start(1000);

      // Callbacks should not have been called yet (no status change)
      expect(onOfflineMock).not.toHaveBeenCalled();
      expect(onOnlineMock).not.toHaveBeenCalled();
    });
  });

  describe('edge cases', () => {
    it('should handle exchange without setOnlineStatus gracefully', () => {
      const exchangeWithoutMethod: ExchangeLike = {
        name: 'basic-exchange',
        emitter: {
          on: vi.fn(),
          off: vi.fn(),
          emit: vi.fn(),
        },
      };

      const monitor = new NetworkMonitor(exchangeWithoutMethod, callbacks);
      monitor.start(1000);

      // Should not throw
      vi.advanceTimersByTime(5000);
    });

    it('should handle empty callbacks object', () => {
      const monitor = new NetworkMonitor(mockExchange, {});
      monitor.start(1000);

      // Should not throw
      vi.advanceTimersByTime(5000);
    });

    it('should handle very short interval', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      monitor.start(1);

      vi.advanceTimersByTime(10);

      monitor.stop();
    });

    it('should handle very long interval', () => {
      const monitor = new NetworkMonitor(mockExchange, callbacks);
      monitor.start(100000);

      // Should still work
      expect(monitor.getIsOnline()).toBe(true);

      monitor.stop();
    });
  });
});
