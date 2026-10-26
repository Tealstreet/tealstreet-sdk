import type { Dayjs } from 'dayjs';

import axios from 'axios';
import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';

import { isNodeOrBunRuntime } from './runtime';

dayjs.extend(utc);

// Imported at call time: the transport module imports this one (via accessToken).
const bybitTimeUrl = async (): Promise<string> =>
  (await import('../transport/registry')).withPublicCorsProxy('https://api.bybit.com/v5/market/time');

export const TIME_SYNC_INTERVAL_MS = 300_000;

type TimeSyncListener = (timeMs: number) => void;

// A host-provided clock (epoch ms, or undefined on failure). Without one the
// clock syncs to venue time, then falls back to the local clock.
export type ServerTimeSource = () => Promise<number | undefined>;

class VirtualClock {
  started = false;
  private timeoutId: ReturnType<typeof setTimeout> | null = null;

  private server: Dayjs | null = null;
  private client: Dayjs | null = null;
  // Cached server-vs-local offset in milliseconds. Maintained alongside
  // `server` / `client` so the hot-path `getCurrentTimeMs` can be a
  // plain `Date.now() + offsetMs` instead of allocating a fresh dayjs
  // per call. Both `dayjs.utc()` and `dayjs.add()` allocate; called
  // from `mapOrder` / `mapPosition` / `lastUpdate` writes / etc.
  // (~209 callsites in safe-cex), this was steady allocator pressure.
  private offsetMs = 0;
  private syncInFlight: Promise<void> | null = null;
  private serverTimeSource: ServerTimeSource | undefined;
  private readonly syncListeners = new Set<TimeSyncListener>();

  start = async () => {
    if (!this.started) {
      this.started = true;
      await this.syncClientServerTime();
      this.scheduleNextSync();
    } else if (this.syncInFlight) {
      await this.syncInFlight;
    }
  };

  stop = () => {
    if (this.timeoutId) {
      clearTimeout(this.timeoutId);
      this.timeoutId = null;
    }
    this.started = false;
  };

  setServerTimeSource = (source: ServerTimeSource | undefined) => {
    this.serverTimeSource = source;
  };

  private fetchFromServerTimeSource = async (): Promise<Dayjs | undefined> => {
    try {
      const serverMs = await this.serverTimeSource?.();
      return serverMs != null && Number.isFinite(serverMs) ? dayjs.utc(serverMs) : undefined;
    } catch {
      return undefined;
    }
  };

  private fetchVenueTime = async (): Promise<Dayjs | undefined> => {
    try {
      const {
        data: { time },
      } = await axios.get(await bybitTimeUrl(), { timeout: 5000 });
      if (time != null) {
        return dayjs(time);
      }
    } catch {
      // ignore
    }
    return undefined;
  };

  private notifyTimeSync = () => {
    const timeMs = this.getCurrentTimeMs();
    for (const listener of this.syncListeners) {
      listener(timeMs);
    }
  };

  subscribeToTimeSync = (listener: TimeSyncListener) => {
    this.syncListeners.add(listener);
    return () => this.syncListeners.delete(listener);
  };

  // Browser workers receive the main-thread clock through this method instead
  // of each polling a public endpoint. The host tab therefore makes one time
  // request regardless of the number of exchange accounts it has open.
  syncFromServerTime = (timeMs: number) => {
    const client = dayjs.utc();
    const server = dayjs.utc(timeMs);

    if (Math.abs(server.diff(client, 'milliseconds')) < 1000) {
      this.server = null;
      this.client = null;
      this.offsetMs = 0;
    } else {
      this.server = server;
      this.client = client;
      this.offsetMs = server.valueOf() - client.valueOf();
    }
  };

  private scheduleNextSync = () => {
    if (!this.started) return;

    const scheduledAt = Date.now();

    this.timeoutId = setTimeout(async () => {
      // Sleep detection: if the timer fired much later than expected,
      // the device likely slept. Clear stale cached time before re-syncing.
      if (Date.now() - scheduledAt > 90_000) {
        this.server = null;
        this.client = null;
        this.offsetMs = 0;
      }
      await this.syncClientServerTime();
      this.scheduleNextSync();
    }, TIME_SYNC_INTERVAL_MS);
    // Lets a short-lived Node process exit; the browser's numeric id has no unref.
    (this.timeoutId as { unref?: () => void } | undefined)?.unref?.();
  };

  getCurrentTime = () => {
    if (this.server && this.client) {
      const now = dayjs.utc();
      const nowClientDiff = now.diff(this.client, 'milliseconds');
      return this.server.add(nowClientDiff, 'milliseconds');
    }

    return dayjs.utc();
  };

  // Hot path: do NOT route through `getCurrentTime` / dayjs. The
  // arithmetic identity is:
  //   server.add(now - client, 'ms').valueOf()
  //     = serverMs + (nowMs - clientMs)
  //     = nowMs + (serverMs - clientMs)
  //     = Date.now() + offsetMs
  // When unsynced, `offsetMs` is 0. Equivalent to the prior
  // `dayjs.utc().valueOf()` minus the per-call dayjs allocation.
  getCurrentTimeMs = () => Date.now() + this.offsetMs;

  public syncClientServerTime = () => {
    if (this.syncInFlight) {
      return this.syncInFlight;
    }

    this.syncInFlight = this.syncClientServerTimeInner().finally(() => {
      this.syncInFlight = null;
    });

    return this.syncInFlight;
  };

  private syncClientServerTimeInner = async () => {
    if (isNodeOrBunRuntime()) {
      // A server keeps its own clock; one shared offset would move every user's.
      this.server = null;
      this.client = null;
      this.offsetMs = 0;
      return;
    }
    // A browser worker (no `window`) keeps the offset the main thread pushes or a
    // venue correction set; zeroing it here undid both every 5 minutes.
    if (typeof window === 'undefined') return;

    this.syncFromServerTime((await this.fetchServerTime()).valueOf());
    this.notifyTimeSync();
  };

  private fetchServerTime = async () =>
    (await this.fetchFromServerTimeSource()) ?? (await this.fetchVenueTime()) ?? dayjs.utc();

  public syncBybitClientServerTime = async (time?: number) => {
    // A web worker has no `window` either; only a Node/Bun server skips. Testing
    // `window` zeroed the offset in every browser exchange worker instead.
    if (isNodeOrBunRuntime()) {
      this.server = null;
      this.client = null;
      this.offsetMs = 0;

      return;
    }
    // Without a venue time, a browser worker keeps the offset the main thread pushes:
    // fetching here would cost every account's worker a request on each WS reconnect.
    if (!time && typeof window === 'undefined') return;

    this.server = time ? dayjs(time) : await this.fetchBybitTime();
    this.client = dayjs.utc();

    // if the difference is less than 1 second
    // assume the time is correct
    if (Math.abs(this.server.diff(this.client, 'milliseconds')) < 1000) {
      this.server = null;
      this.client = null;
      this.offsetMs = 0;
    } else {
      this.offsetMs = this.server.valueOf() - this.client.valueOf();
    }
  };

  // Bybit-specific sync wants Bybit's actual clock, so venue time leads here.
  private fetchBybitTime = async () =>
    (await this.fetchVenueTime()) ?? (await this.fetchFromServerTimeSource()) ?? dayjs.utc();
}

export const virtualClock = new VirtualClock();
