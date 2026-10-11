import axios from 'axios';
import dayjs from 'dayjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TIME_SYNC_INTERVAL_MS, virtualClock } from './virtualClock';

// Whether this is a Node/Bun server: the Bybit sync skips only there, not in a browser worker.
const runtime = vi.hoisted(() => ({ node: false }));
vi.mock('./runtime', () => ({ isNodeOrBunRuntime: () => runtime.node }));

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
    virtualClock.setServerTimeSource(undefined);
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
    it('zeroes the offset on a Node/Bun server', async () => {
      runtime.node = true;
      virtualClock.syncFromServerTime(Date.now() + 10_000);

      await virtualClock.syncClientServerTime();

      expect((virtualClock as any).offsetMs).toBe(0);
      runtime.node = false;
    });

    // The main thread pushes the offset into a worker; the worker's own sync must not undo it.
    it('keeps a pushed offset in a browser worker, which has no window', async () => {
      const originalWindow = global.window;
      (global as any).window = undefined;
      virtualClock.syncFromServerTime(Date.now() + 10_000);

      await virtualClock.syncClientServerTime();

      expect((virtualClock as any).offsetMs).toBeGreaterThan(9_000);
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
    afterEach(() => {
      runtime.node = false;
    });

    it('does nothing on a Node/Bun server', async () => {
      runtime.node = true;

      await virtualClock.syncBybitClientServerTime(Date.now() + 10_000);

      expect((virtualClock as any).server).toBeNull();
      expect((virtualClock as any).client).toBeNull();
    });

    // A web worker has no `window` either; testing for it zeroed every browser worker's offset.
    it('still syncs in a browser worker, which has no window', async () => {
      const originalWindow = global.window;
      (global as any).window = undefined;

      await virtualClock.syncBybitClientServerTime(Date.now() + 10_000);

      expect((virtualClock as any).offsetMs).toBeGreaterThan(9_000);
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

    it('keeps the pushed offset in a browser worker when given no venue time', async () => {
      const originalWindow = global.window;
      (global as any).window = undefined;
      virtualClock.syncFromServerTime(Date.now() + 5_000);
      vi.mocked(axios.get).mockClear();

      await virtualClock.syncBybitClientServerTime();

      expect(axios.get).not.toHaveBeenCalled();
      expect((virtualClock as any).offsetMs).toBeGreaterThan(4_000);
      (global as any).window = originalWindow;
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

  describe('time source order', () => {
    it('syncs to venue time when the host registered no time source', async () => {
      (global as any).window = {};
      vi.mocked(axios.get).mockResolvedValueOnce({ data: { time: Date.now() + 10_000 } });

      await virtualClock.syncClientServerTime();

      expect(axios.get).toHaveBeenCalledTimes(1);
      expect(String(vi.mocked(axios.get).mock.calls[0][0])).toContain('api.bybit.com/v5/market/time');
      expect((virtualClock as any).offsetMs).toBeGreaterThan(9_000);
      delete (global as any).window;
    });

    it('prefers a registered time source over venue time', async () => {
      (global as any).window = {};
      const source = vi.fn().mockResolvedValue(Date.now() + 20_000);
      virtualClock.setServerTimeSource(source);

      await virtualClock.syncClientServerTime();

      expect(source).toHaveBeenCalledTimes(1);
      expect(axios.get).not.toHaveBeenCalled();
      expect((virtualClock as any).offsetMs).toBeGreaterThan(19_000);
      delete (global as any).window;
    });

    it('falls back to venue time, then the local clock, when the source fails', async () => {
      (global as any).window = {};
      virtualClock.setServerTimeSource(async () => undefined);
      vi.mocked(axios.get).mockRejectedValue(new Error('offline'));

      await virtualClock.syncClientServerTime();

      expect(axios.get).toHaveBeenCalledTimes(1);
      expect((virtualClock as any).offsetMs).toBe(0);
      delete (global as any).window;
    });

    it('treats a throwing source as unavailable', async () => {
      (global as any).window = {};
      virtualClock.setServerTimeSource(async () => {
        throw new Error('host clock broke');
      });
      vi.mocked(axios.get).mockResolvedValueOnce({ data: { time: Date.now() + 10_000 } });

      await expect(virtualClock.syncClientServerTime()).resolves.toBeUndefined();
      expect((virtualClock as any).offsetMs).toBeGreaterThan(9_000);
      delete (global as any).window;
    });

    it('bybit sync leads with venue time and falls back to the source', async () => {
      (global as any).window = {};
      const source = vi.fn().mockResolvedValue(Date.now() + 30_000);
      virtualClock.setServerTimeSource(source);
      vi.mocked(axios.get).mockRejectedValue(new Error('geo-blocked'));

      await virtualClock.syncBybitClientServerTime();

      expect(axios.get).toHaveBeenCalledTimes(1);
      expect(source).toHaveBeenCalledTimes(1);
      expect((virtualClock as any).offsetMs).toBeGreaterThan(29_000);
      delete (global as any).window;
    });
  });
});
