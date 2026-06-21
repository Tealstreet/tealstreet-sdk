import type { Dayjs } from 'dayjs';

import axios from 'axios';
import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';

import { PUBLIC_CORS_PROXY } from './transportResolution';

dayjs.extend(utc);

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
    }, 60_000);
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

  public syncClientServerTime = async () => {
    if (typeof window === 'undefined') {
      // assume we are running onto a server
      // this should have a better time sync
      this.server = null;
      this.client = null;
      this.offsetMs = 0;

      return;
    }

    this.server = await this.fetchServerTime();
    this.client = dayjs.utc();

    // if the difference is less than 5 seconds
    // assume the time is correct
    if (Math.abs(this.server.diff(this.client, 'milliseconds')) < 1000) {
      this.server = null;
      this.client = null;
      this.offsetMs = 0;
    } else {
      this.offsetMs = this.server.valueOf() - this.client.valueOf();
    }
  };

  private fetchServerTime = async () => {
    // Lead with our own same-origin /api/time: no CORS, no geo-block, always
    // reachable wherever the app is served. Offset is only applied when drift
    // exceeds 1s, so an NTP-synced edge clock is more than accurate enough.
    // (Previously this led with a direct api.bybit.com call, which 403s from
    // any region Bybit's CloudFront blocks — spamming a failed request +
    // console error on every page load for those users.)
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

    try {
      const {
        data: { time },
      } = await axios.get(`${PUBLIC_CORS_PROXY}/https://api.bybit.com/v5/market/time`, { timeout: 5000 });
      if (time != null) {
        return dayjs(time);
      }
    } catch {
      // ignore
    }

    try {
      const { data } = await axios.get('https://worldtimeapi.org/api/timezone/etc/utc', { timeout: 500 });
      return dayjs(data.utc_datetime);
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

    // if the difference is less than 5 seconds
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
      } = await axios.get(`${PUBLIC_CORS_PROXY}/https://api.bybit.com/v5/market/time`, { timeout: 5000 });
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
