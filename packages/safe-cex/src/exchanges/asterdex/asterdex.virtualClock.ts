import type { Dayjs } from 'dayjs';
import { ASTERDEX_ENDPOINTS, type AsterDEXAPIVersion } from './asterdex.types';

import axios from 'axios';
import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';

import { withPublicCorsProxy } from '../../transport/registry';

dayjs.extend(utc);

/**
 * AsterDEX-specific virtual clock for time synchronization
 * This is separate from the global virtualClock to handle AsterDEX's specific server time
 */
class AsterDEXVirtualClock {
  private server: Dayjs | null = null;
  private client: Dayjs | null = null;
  private syncInterval: NodeJS.Timeout | null = null;
  private syncInFlight: Promise<void> | null = null;
  private lifecycleVersion = 0;
  private preferredApiVersion: AsterDEXAPIVersion = 'v1';

  /**
   * Start the virtual clock and sync with AsterDEX server time
   */
  start = async (testnet: boolean = false, preferredApiVersion: AsterDEXAPIVersion = 'v1') => {
    // A start may be awaiting its initial sync when stop()/dispose() runs.
    // Keep the generation from this invocation so it cannot install a timer
    // after that lifecycle has ended (or after a newer start superseded it).
    const lifecycleVersion = ++this.lifecycleVersion;
    this.preferredApiVersion = preferredApiVersion;
    await this.syncClientServerTime(testnet, preferredApiVersion);

    if (lifecycleVersion !== this.lifecycleVersion) return;

    // Sync every 5 minutes
    if (this.syncInterval) {
      clearInterval(this.syncInterval);
    }
    this.syncInterval = setInterval(() => {
      if (lifecycleVersion !== this.lifecycleVersion) return;
      void this.syncClientServerTime(testnet, preferredApiVersion);
    }, 5 * 60 * 1000);
  };

  /**
   * Stop the virtual clock and clear sync interval
   */
  stop = () => {
    this.lifecycleVersion += 1;
    if (this.syncInterval) {
      clearInterval(this.syncInterval);
      this.syncInterval = null;
    }
    this.server = null;
    this.client = null;
  };

  /**
   * Get current time adjusted for server offset
   */
  getCurrentTime = (): Dayjs => {
    if (this.server && this.client) {
      const now = dayjs.utc();
      const nowClientDiff = now.diff(this.client, 'milliseconds');
      return this.server.add(nowClientDiff, 'milliseconds');
    }

    return dayjs.utc();
  };

  /**
   * Get current time in milliseconds adjusted for server offset
   */
  getCurrentTimeMs = (): number => this.getCurrentTime().valueOf();

  /**
   * Sync client time with AsterDEX server time
   */
  public syncClientServerTime = async (
    testnet: boolean = false,
    preferredApiVersion: AsterDEXAPIVersion = this.preferredApiVersion,
  ): Promise<void> => {
    if (this.syncInFlight) return this.syncInFlight;

    const lifecycleVersion = this.lifecycleVersion;
    this.syncInFlight = this.syncClientServerTimeInner(testnet, preferredApiVersion, lifecycleVersion).finally(() => {
      this.syncInFlight = null;
    });
    return this.syncInFlight;
  };

  private syncClientServerTimeInner = async (
    testnet: boolean,
    preferredApiVersion: AsterDEXAPIVersion,
    lifecycleVersion: number,
  ): Promise<void> => {
    this.preferredApiVersion = preferredApiVersion;
    // Axios has browser-worker and non-window runtime transports. Do not use
    // `window` as a runtime proxy: WebWorkers deliberately do not expose it.
    const server = await this.fetchServerTime(testnet, preferredApiVersion);
    if (!server || lifecycleVersion !== this.lifecycleVersion) return;

    this.server = server;
    this.client = dayjs.utc();

    // Always apply the offset, even if small. This ensures signatures use
    // Aster's clock in WebWorkers as well as browser windows.
    const offset = this.server.diff(this.client, 'milliseconds');
    console.log(`[AsterDEX] Time sync: Server time offset is ${offset}ms`);
  };

  /**
   * Fetch server time from AsterDEX API
   */
  private getTimePathCandidates = (preferredApiVersion: AsterDEXAPIVersion): string[] => {
    return preferredApiVersion === 'v3' ? ['/fapi/v3/time', '/fapi/v1/time'] : ['/fapi/v1/time', '/fapi/v3/time'];
  };

  private fetchServerTime = async (
    testnet: boolean = false,
    preferredApiVersion: AsterDEXAPIVersion = this.preferredApiVersion,
  ): Promise<Dayjs | null> => {
    const baseUrl = testnet ? ASTERDEX_ENDPOINTS.REST.TESTNET : ASTERDEX_ENDPOINTS.REST.PRODUCTION;
    const timePathCandidates = this.getTimePathCandidates(preferredApiVersion);

    for (const timePath of timePathCandidates) {
      try {
        const { data } = await axios.get(`${baseUrl}${timePath}`, { timeout: 5000 });
        const serverTime = this.parseServerTime(data?.serverTime);
        if (serverTime !== undefined) {
          return dayjs.utc(serverTime);
        }
      } catch {
        // Ignore error and continue to next candidate
      }
    }

    // Try through proxy
    for (const timePath of timePathCandidates) {
      try {
        const { data } = await axios.get(withPublicCorsProxy(`${baseUrl}${timePath}`), { timeout: 5000 });
        const serverTime = this.parseServerTime(data?.serverTime);
        if (serverTime !== undefined) {
          return dayjs.utc(serverTime);
        }
      } catch {
        // Ignore error and continue to next candidate
      }
    }

    // Keep the last known-good offset on failed sync instead of converting a
    // transient transport failure into a local-clock reset.
    return null;
  };

  private parseServerTime = (value: unknown): number | undefined => {
    if (typeof value !== 'number' && typeof value !== 'string') return undefined;
    if (typeof value === 'string' && value.trim() === '') return undefined;
    const parsed = Number(value);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : undefined;
  };

  /**
   * Get the current time offset in milliseconds
   */
  public getTimeOffset = (): number => {
    if (this.server && this.client) {
      return this.server.diff(this.client, 'milliseconds');
    }
    return 0;
  };

  /**
   * Force sync time and return the offset
   */
  public forceSyncAndGetOffset = async (
    testnet: boolean = false,
    preferredApiVersion: AsterDEXAPIVersion = this.preferredApiVersion,
  ): Promise<number> => {
    await this.syncClientServerTime(testnet, preferredApiVersion);
    return this.getTimeOffset();
  };

  /**
   * Check if time sync is needed based on last sync time
   */
  public needsSync = (): boolean => {
    // If we don't have a sync, we need one
    if (!this.server || !this.client) {
      return true;
    }

    // If last sync was more than 5 minutes ago, we need a new sync
    const now = dayjs.utc();
    const lastSyncAge = now.diff(this.client, 'milliseconds');
    return lastSyncAge > 5 * 60 * 1000;
  };
}

export const asterDEXVirtualClock = new AsterDEXVirtualClock();
