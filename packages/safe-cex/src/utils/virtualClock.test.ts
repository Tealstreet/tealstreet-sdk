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
    (virtualClock as any).receivedTimeSync = false;
    (virtualClock as any).syncInFlight = null;
    (virtualClock as any).lastAfterClockStepSync = null;
    virtualClock.setServerTimeSource(undefined);
  });

  afterEach(() => {
    virtualClock.stop();
    vi.restoreAllMocks();
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
    it.each([
      { afterClockStep: false, fail: false, stop: false },
      { afterClockStep: true, fail: false, stop: false },
      { afterClockStep: true, fail: true, stop: false },
      { afterClockStep: true, fail: false, stop: true },
    ])(
      'retains in-flight steps until completion (initial=$afterClockStep, fail=$fail, stop=$stop)',
      async ({ afterClockStep, fail, stop }) => {
        const originalWindow = global.window;
        (global as any).window = {};
        let elapsed = 0;
        vi.spyOn(performance, 'now').mockImplementation(() => elapsed);
        let finish!: (time: number | undefined) => void;
        const source = vi
          .fn()
          .mockImplementationOnce(
            () =>
              new Promise<number | undefined>((resolve) => {
                finish = resolve;
              }),
          )
          .mockImplementation(async () => Date.now());
        vi.mocked(axios.get).mockRejectedValue(new Error('fixture venue unavailable'));
        virtualClock.setServerTimeSource(source);
        try {
          const pending = virtualClock.syncClientServerTime({ afterClockStep });
          for (let i = 0; i < 3; i++) {
            expect(virtualClock.syncClientServerTime({ afterClockStep: true })).toBe(pending);
          }
          expect(source).toHaveBeenCalledOnce();
          if (stop) virtualClock.stop();
          finish(fail ? undefined : Date.now());
          await pending;
          await vi.advanceTimersByTimeAsync(0);
          expect(source).toHaveBeenCalledTimes(afterClockStep || stop ? 1 : 2);
          elapsed = 14_999;
          await vi.advanceTimersByTimeAsync(14_999);
          expect(source).toHaveBeenCalledTimes(afterClockStep || stop ? 1 : 2);
          elapsed = 15_000;
          await vi.advanceTimersByTimeAsync(1);
          expect(source).toHaveBeenCalledTimes(stop ? 1 : 2);
          expect(vi.getTimerCount()).toBe(0);
        } finally {
          (global as any).window = originalWindow;
        }
      },
    );

    it.each([false, true])('recovers real-provider sampling after a backward step (overlap=%s)', async (overlap) => {
      const { tealstreetTimeSource } = await import('../transport/tealstreet/tealstreetTimeSource');
      const originalWindow = global.window;
      (global as any).window = {};
      const epoch = 1_760_000_000_000;
      vi.setSystemTime(epoch);
      let elapsed = 0;
      let stepDuringResponse = false;
      vi.spyOn(performance, 'now').mockImplementation(() => elapsed);
      vi.mocked(axios.get).mockImplementation(async () => {
        await Promise.resolve();
        if (stepDuringResponse) {
          stepDuringResponse = false;
          vi.setSystemTime(Date.now() - 600_000);
          void virtualClock.syncClientServerTime({ afterClockStep: true });
        }
        return { data: { unixMs: epoch + elapsed } };
      });
      const source = vi.fn(tealstreetTimeSource);
      virtualClock.setServerTimeSource(source);
      try {
        await virtualClock.syncClientServerTime({ afterClockStep: true });
        elapsed = 1_000;
        await vi.advanceTimersByTimeAsync(1_000);
        vi.setSystemTime(Date.now() - 600_000);
        await virtualClock.syncClientServerTime({ afterClockStep: true });
        stepDuringResponse = overlap;
        elapsed = 15_000;
        await vi.advanceTimersByTimeAsync(14_000);
        expect(source).toHaveBeenCalledTimes(2);
        elapsed = 29_999;
        await vi.advanceTimersByTimeAsync(14_999);
        expect(source).toHaveBeenCalledTimes(2);
        elapsed = 30_000;
        await vi.advanceTimersByTimeAsync(1);
        expect.soft(virtualClock.getCurrentTimeMs()).toBe(epoch + elapsed);
        expect.soft(source).toHaveBeenCalledTimes(overlap ? 3 : 2);
        expect(vi.getTimerCount()).toBe(0);
      } finally {
        (global as any).window = originalWindow;
      }
    });

    it('coalesces suppressed clock steps and cancels their deferred resync on stop', async () => {
      const originalWindow = global.window;
      (global as any).window = {};
      let elapsed = 0;
      vi.spyOn(performance, 'now').mockImplementation(() => elapsed);
      const source = vi.fn(async () => Date.now());
      virtualClock.setServerTimeSource(source);
      try {
        await virtualClock.syncClientServerTime({ afterClockStep: true });
        elapsed = 1_000;
        for (let i = 0; i < 3; i++) await virtualClock.syncClientServerTime({ afterClockStep: true });
        expect(vi.getTimerCount()).toBe(1);
        expect(source).toHaveBeenCalledOnce();
        virtualClock.stop();
        elapsed = 15_000;
        await vi.advanceTimersByTimeAsync(15_000);
        expect(source).toHaveBeenCalledOnce();
      } finally {
        (global as any).window = originalWindow;
      }
    });

    it('rechecks the monotonic cooldown when a deferred timer fires early', async () => {
      const originalWindow = global.window;
      (global as any).window = {};
      let elapsed = 0;
      vi.spyOn(performance, 'now').mockImplementation(() => elapsed);
      const source = vi.fn(async () => Date.now());
      virtualClock.setServerTimeSource(source);
      try {
        await virtualClock.syncClientServerTime({ afterClockStep: true });
        elapsed = 1_000;
        await virtualClock.syncClientServerTime({ afterClockStep: true });
        await vi.advanceTimersByTimeAsync(14_000);
        expect(source).toHaveBeenCalledOnce();
        elapsed = 15_000;
        await vi.advanceTimersByTimeAsync(14_000);
        expect(source).toHaveBeenCalledTimes(2);
        expect(vi.getTimerCount()).toBe(0);
      } finally {
        (global as any).window = originalWindow;
      }
    });

    it('rate limits completed clock-step syncs using monotonic elapsed time only', async () => {
      const originalWindow = global.window;
      (global as any).window = {};
      let elapsed = 0;
      vi.spyOn(performance, 'now').mockImplementation(() => elapsed);
      const source = vi.fn(async () => Date.now() + 5_000);
      virtualClock.setServerTimeSource(source);
      try {
        await virtualClock.syncClientServerTime({ afterClockStep: true });
        for (const jump of [600_000, -1_200_000, 1_800_000]) {
          vi.setSystemTime(Date.now() + jump);
          elapsed += 1_000;
          await virtualClock.syncClientServerTime({ afterClockStep: true });
        }
        elapsed = 14_999;
        await virtualClock.syncClientServerTime({ afterClockStep: true });
        expect(source).toHaveBeenCalledOnce();
        elapsed = 15_000;
        await virtualClock.syncClientServerTime({ afterClockStep: true });
        expect(source).toHaveBeenCalledTimes(2);
        await virtualClock.syncClientServerTime();
        expect(source).toHaveBeenCalledTimes(3); // Routine sync is independent.
      } finally {
        (global as any).window = originalWindow;
      }
    });

    it('limits Node clock-step notifications across all realm subscribers', async () => {
      runtime.node = true;
      const first = vi.fn();
      const second = vi.fn();
      const unsubscribeFirst = virtualClock.subscribeToTimeSync(first);
      const unsubscribeSecond = virtualClock.subscribeToTimeSync(second);
      try {
        await virtualClock.syncClientServerTime({ afterClockStep: true });
        await virtualClock.syncClientServerTime({ afterClockStep: true });
        expect(first).toHaveBeenCalledOnce();
        expect(second).toHaveBeenCalledOnce();
        expect(axios.get).not.toHaveBeenCalled();
      } finally {
        unsubscribeFirst();
        unsubscribeSecond();
        runtime.node = false;
      }
    });

    it('keeps the prior offset and publishes nothing when clock-step sources both fail', async () => {
      const originalWindow = global.window;
      (global as any).window = undefined;
      const source = vi.fn().mockRejectedValue(new Error('fixture source unavailable'));
      virtualClock.setServerTimeSource(source);
      vi.mocked(axios.get).mockRejectedValue(new Error('fixture venue unavailable'));
      virtualClock.syncFromServerTime(Date.now() + 10_000);
      const offset = (virtualClock as any).offsetMs;
      const observed = vi.fn();
      const unsubscribe = virtualClock.subscribeToTimeSync(observed);
      try {
        await virtualClock.syncClientServerTime({ afterClockStep: true });
        expect(source).toHaveBeenCalledOnce();
        expect(axios.get).toHaveBeenCalledOnce();
        expect((virtualClock as any).offsetMs).toBe(offset);
        expect(observed).not.toHaveBeenCalled();
        await virtualClock.syncClientServerTime({ afterClockStep: true });
        expect(source).toHaveBeenCalledOnce(); // Failed attempts cannot storm either.
        expect(axios.get).toHaveBeenCalledOnce();
      } finally {
        unsubscribe();
        (global as any).window = originalWindow;
      }
    });

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

      virtualClock.syncFromServerTime(Date.now());
      await virtualClock.start();

      expect(virtualClock.started).toBe(true);

      (global as any).window = originalWindow;
    });

    it('should not start twice if already started', async () => {
      const originalWindow = global.window;
      (global as any).window = undefined;

      virtualClock.syncFromServerTime(Date.now());
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

  it('publishes the existing Node OS-clock refresh to protocol-clock subscribers without venue I/O', async () => {
    runtime.node = true;
    const observed = vi.fn();
    const unsubscribe = virtualClock.subscribeToTimeSync(observed);
    try {
      await virtualClock.syncClientServerTime();
      expect(observed).toHaveBeenLastCalledWith(Date.now());
      vi.setSystemTime(Date.now() + 162 * 86_400_000);
      await virtualClock.syncClientServerTime();
      expect(observed).toHaveBeenCalledTimes(2);
      expect(observed).toHaveBeenLastCalledWith(Date.now());
      expect(axios.get).not.toHaveBeenCalled();
    } finally {
      unsubscribe();
      runtime.node = false;
    }
  });

  it('waits for the first host sync in a browser worker, with one notification', async () => {
    const originalWindow = global.window;
    (global as any).window = undefined;
    let done = false;
    const start = virtualClock.start().then(() => {
      done = true;
    });
    await Promise.resolve();
    expect(done).toBe(false);
    const observed = vi.fn();
    const unsubscribe = virtualClock.subscribeToTimeSync(observed);
    const utc = Date.now() + 162 * 86_400_000;
    virtualClock.syncFromServerTime(utc);
    await start;
    expect(done).toBe(true);
    expect(virtualClock.getCurrentTimeMs()).toBe(utc);
    expect(observed).toHaveBeenCalledExactlyOnceWith(utc);
    unsubscribe();
    (global as any).window = originalWindow;
  });

  it('fails startup clearly rather than anchoring an unsynchronized worker clock', async () => {
    const originalWindow = global.window;
    (global as any).window = undefined;
    const refused = expect(virtualClock.start()).rejects.toThrow('Clock synchronization did not arrive from the host');
    await vi.advanceTimersByTimeAsync(30_000);
    await refused;
    expect((virtualClock as any).syncListeners.size).toBe(0);
    (global as any).window = originalWindow;
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
