import axios from 'axios';
import dayjs from 'dayjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TIME_SYNC_INTERVAL_MS, virtualClock } from './virtualClock';

// Mock axios before importing virtualClock
vi.mock('axios', () => ({
  default: {
    get: vi.fn(),
  },
}));

describe('virtualClock', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();

    // Reset virtualClock state
    virtualClock.stop();
    (virtualClock as any).started = false;
    (virtualClock as any).server = null;
    (virtualClock as any).client = null;
    (virtualClock as any).offsetMs = 0;
    (virtualClock as any).syncInFlight = null;
    delete process.env.NEXT_PUBLIC_TIME_API_ORIGIN;
  });

  afterEach(() => {
    virtualClock.stop();
    vi.useRealTimers();
  });

  describe('initial state', () => {
    it('should have started as false initially', () => {
      expect(virtualClock.started).toBe(false);
    });
  });

  describe('getCurrentTime', () => {
    it('should return dayjs UTC time when server/client not synced', () => {
      const time = virtualClock.getCurrentTime();
      expect(time).toBeDefined();
      expect(typeof time.valueOf()).toBe('number');
    });

    it('should return adjusted time when server/client are synced', () => {
      // Manually set server and client times
      const serverTime = dayjs.utc().add(5, 'seconds');
      const clientTime = dayjs.utc();

      (virtualClock as any).server = serverTime;
      (virtualClock as any).client = clientTime;

      const time = virtualClock.getCurrentTime();
      // The result should be approximately 5 seconds ahead
      const diff = time.diff(dayjs.utc(), 'seconds');
      expect(diff).toBeGreaterThanOrEqual(4);
      expect(diff).toBeLessThanOrEqual(6);
    });
  });

  describe('getCurrentTimeMs', () => {
    it('should return a number representing milliseconds', () => {
      const ms = virtualClock.getCurrentTimeMs();
      expect(typeof ms).toBe('number');
      expect(ms).toBeGreaterThan(0);
    });
  });

  describe('stop', () => {
    it('should set started to false', () => {
      (virtualClock as any).started = true;
      virtualClock.stop();
      expect(virtualClock.started).toBe(false);
    });

    it('should clear timeout if exists', () => {
      (virtualClock as any).timeoutId = setTimeout(() => {}, 1000);
      virtualClock.stop();
      expect((virtualClock as any).timeoutId).toBeNull();
    });

    it('should handle stop when no timeoutId exists', () => {
      (virtualClock as any).timeoutId = null;
      virtualClock.stop();
      expect((virtualClock as any).timeoutId).toBeNull();
      expect(virtualClock.started).toBe(false);
    });
  });

  describe('syncClientServerTime', () => {
    it('should set server and client to null when window is undefined', async () => {
      // In Node.js environment, window is undefined
      const originalWindow = global.window;
      (global as any).window = undefined;

      await virtualClock.syncClientServerTime();

      expect((virtualClock as any).server).toBeNull();
      expect((virtualClock as any).client).toBeNull();

      (global as any).window = originalWindow;
    });

    it('should sync time when server time differs significantly from client', async () => {
      // Mock window to exist
      (global as any).window = {};

      // Mock axios to return a time 10 seconds ahead
      const serverTime = Date.now() + 10000;
      vi.mocked(axios.get).mockResolvedValueOnce({
        data: { time: serverTime },
      });

      await virtualClock.syncClientServerTime();

      // Server and client should be set because difference > 1000ms
      expect((virtualClock as any).server).not.toBeNull();
      expect((virtualClock as any).client).not.toBeNull();

      delete (global as any).window;
    });

    it('should not sync when server time is close to client time', async () => {
      // Mock window to exist
      (global as any).window = {};

      // Mock axios to return a time very close to now
      const serverTime = Date.now() + 100; // Only 100ms difference
      vi.mocked(axios.get).mockResolvedValueOnce({
        data: { time: serverTime },
      });

      await virtualClock.syncClientServerTime();

      // Server and client should remain null because difference < 1000ms
      expect((virtualClock as any).server).toBeNull();
      expect((virtualClock as any).client).toBeNull();

      delete (global as any).window;
    });
  });

  describe('syncBybitClientServerTime', () => {
    it('should set server and client to null when window is undefined', async () => {
      const originalWindow = global.window;
      (global as any).window = undefined;

      await virtualClock.syncBybitClientServerTime();

      expect((virtualClock as any).server).toBeNull();
      expect((virtualClock as any).client).toBeNull();

      (global as any).window = originalWindow;
    });

    it('should use provided time parameter when given', async () => {
      (global as any).window = {};

      const serverTime = Date.now() + 10000;
      await virtualClock.syncBybitClientServerTime(serverTime);

      // Server should be set since difference is > 1000ms
      expect((virtualClock as any).server).not.toBeNull();

      delete (global as any).window;
    });

    it('should not sync when time difference is small', async () => {
      (global as any).window = {};

      const serverTime = Date.now() + 100; // Only 100ms difference
      await virtualClock.syncBybitClientServerTime(serverTime);

      // Server and client should be null because difference < 1000ms
      expect((virtualClock as any).server).toBeNull();
      expect((virtualClock as any).client).toBeNull();

      delete (global as any).window;
    });

    it('should fetch from bybit API when time not provided', async () => {
      (global as any).window = {};

      const serverTime = Date.now() + 5000;
      vi.mocked(axios.get).mockResolvedValueOnce({
        data: { time: serverTime },
      });

      await virtualClock.syncBybitClientServerTime();

      expect(axios.get).toHaveBeenCalled();

      delete (global as any).window;
    });
  });

  describe('start', () => {
    it('should set started to true', async () => {
      // Mock window to exist (for syncClientServerTime)
      const originalWindow = global.window;
      (global as any).window = undefined;

      await virtualClock.start();

      expect(virtualClock.started).toBe(true);

      (global as any).window = originalWindow;
    });

    it('should not start twice if already started', async () => {
      const originalWindow = global.window;
      (global as any).window = undefined;

      await virtualClock.start();
      const startedFirst = virtualClock.started;

      // Try starting again
      await virtualClock.start();

      expect(startedFirst).toBe(true);
      expect(virtualClock.started).toBe(true);

      (global as any).window = originalWindow;
    });

    it('resyncs every five minutes', async () => {
      const originalWindow = global.window;
      (global as any).window = {};
      vi.mocked(axios.get).mockResolvedValue({ data: { time: Date.now() + 10_000 } });

      await virtualClock.start();
      expect(axios.get).toHaveBeenCalledTimes(1);

      await vi.advanceTimersByTimeAsync(TIME_SYNC_INTERVAL_MS - 1);
      expect(axios.get).toHaveBeenCalledTimes(1);

      await vi.advanceTimersByTimeAsync(1);
      expect(axios.get).toHaveBeenCalledTimes(2);

      (global as any).window = originalWindow;
    });
  });

  describe('dedicated time origin', () => {
    it('uses the configured public endpoint instead of the Vercel route', async () => {
      (global as any).window = {};
      process.env.NEXT_PUBLIC_TIME_API_ORIGIN = 'https://time.tealstreet.io///';
      vi.mocked(axios.get).mockResolvedValueOnce({ data: { time: new Date().toISOString(), unixMs: Date.now() } });

      await virtualClock.syncClientServerTime();

      expect(axios.get).toHaveBeenCalledWith('https://time.tealstreet.io/v1/time', { timeout: 5000 });
      delete (global as any).window;
    });

    it('falls back to the local clock after the two maintained sources fail', async () => {
      (global as any).window = {};
      vi.mocked(axios.get).mockRejectedValue(new Error('offline'));

      await virtualClock.syncClientServerTime();

      expect(axios.get).toHaveBeenCalledTimes(2);
      expect(vi.mocked(axios.get).mock.calls.some(([url]) => String(url).includes('worldtimeapi.org'))).toBe(false);
      delete (global as any).window;
    });
  });

  describe('private getBaseUrl', () => {
    it('should return empty string when self is undefined', () => {
      const getBaseUrl = (virtualClock as any).getBaseUrl;
      // In Node.js environment, self is undefined
      expect(typeof getBaseUrl()).toBe('string');
    });
  });
});
