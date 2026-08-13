import type { Dayjs } from 'dayjs';

import axios from 'axios';
import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';

import { getPublicCorsProxy } from './transportResolution';

dayjs.extend(utc);

export const TIME_SYNC_INTERVAL_MS = 300_000;

type TimeSyncListener = (timeMs: number) => void;

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
  private readonly syncListeners = new Set<TimeSyncListener>();

  start = async () => {
    if (!this.started) {
      this.started = true;
      await this.syncClientServerTime();
      this.scheduleNextSync();
    }
  };

  stop = () => {
    if (this.timeoutId) {
      clearTimeout(this.timeoutId);
      this.timeoutId = null;
    }
    this.started = false;
  };

  private getBaseUrl = (): string => {
    return typeof self !== 'undefined' && self.location?.origin ? self.location.origin : '';
  };

  /**
   * Accepts either a bare origin for the dedicated time service, which serves
   * `/v1/time`, or a full endpoint URL.
   *
   * The full-URL form exists for the native app. It has no usable same-origin
   * to fall back to — `self.location.origin` there is the Metro dev server —
   * so it must name a route explicitly, and the route it wants is the edge
   * `/api/time` rather than the CloudFront-fronted service. Measured: ~750ms
   * against the dedicated host versus ~70ms against the edge route, on a call
   * that sits on the boot path.
   */
  private getTimeEndpoint = (): string => {
    const configured = process.env.NEXT_PUBLIC_TIME_API_ORIGIN?.trim().replace(/\/+$/, '');
    if (!configured) return `${this.getBaseUrl()}/api/time`;
    return /\/time$/.test(configured) ? configured : `${configured}/v1/time`;
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
    if (typeof window === 'undefined') {
      // assume we are running onto a server
      // this should have a better time sync
      this.server = null;
      this.client = null;
      this.offsetMs = 0;

      return;
    }

    this.syncFromServerTime((await this.fetchServerTime()).valueOf());
    this.notifyTimeSync();
  };

  private fetchServerTime = async () => {
    // Lead with the dedicated public time service when configured, otherwise
    // keep the same-origin route as a rollout-safe fallback. The dedicated
    // service is CloudFront-fronted and makes this hot endpoint independent of
    // the Vercel deployment that serves the UI.
    try {
      const requestedAt = Date.now();
      const {
        data: { time, unixMs },
      } = await axios.get(this.getTimeEndpoint(), { timeout: 5000 });
      const receivedAt = Date.now();
      const serverMs = typeof unixMs === 'number' ? unixMs : dayjs.utc(time).valueOf();
      if (Number.isFinite(serverMs)) {
        // Account for half the request round-trip. The endpoint records its
        // timestamp immediately before responding, so this is conservative
        // while avoiding a systematic network-latency lag.
        return dayjs.utc(serverMs + Math.floor((receivedAt - requestedAt) / 2));
      }
    } catch {
      // ignore
    }

    try {
      const {
        data: { time },
      } = await axios.get(`${getPublicCorsProxy()}/https://api.bybit.com/v5/market/time`, { timeout: 5000 });
      if (time != null) {
        return dayjs(time);
      }
    } catch {
      // ignore
    }

    return dayjs.utc();
  };

  public syncBybitClientServerTime = async (time?: number) => {
    if (typeof window === 'undefined') {
      // assume we are running onto a server
      // this should have a better time sync
      this.server = null;
      this.client = null;
      this.offsetMs = 0;

      return;
    }

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

  private fetchBybitTime = async () => {
    // Bybit-specific sync wants Bybit's actual server time. Route through the
    // JP proxy first: it returns Bybit's real clock via a non-geo-blocked
    // egress, so we get the right value without the direct api.bybit.com call
    // that 403s (CloudFront country block) from blocked regions.
    try {
      const {
        data: { time },
      } = await axios.get(`${getPublicCorsProxy()}/https://api.bybit.com/v5/market/time`, { timeout: 5000 });
      if (time != null) {
        return dayjs(time);
      }
    } catch {
      // ignore
    }

    try {
      const {
        data: { time },
      } = await axios.get(`${this.getBaseUrl()}/api/time`, { timeout: 5000 });
      if (time != null) {
        return dayjs.utc(time);
      }
    } catch {
      // ignore
    }

    return dayjs.utc();
  };
}

export const virtualClock = new VirtualClock();
