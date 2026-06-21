import type { Dayjs } from 'dayjs';
import type { AsterDEXAPIVersion } from './asterdex.types';

import axios from 'axios';
import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';

import { getPublicCorsProxy } from '../../utils/transportResolution';

dayjs.extend(utc);

/**
 * AsterDEX-specific virtual clock for time synchronization
 * This is separate from the global virtualClock to handle AsterDEX's specific server time
 */
class AsterDEXVirtualClock {
  private server: Dayjs | null = null;
  private client: Dayjs | null = null;
  private syncInterval: NodeJS.Timeout | null = null;
  private preferredApiVersion: AsterDEXAPIVersion = 'v1';

  /**
   * Start the virtual clock and sync with AsterDEX server time
   */
  start = async (testnet: boolean = false, preferredApiVersion: AsterDEXAPIVersion = 'v1') => {
    this.preferredApiVersion = preferredApiVersion;
    await this.syncClientServerTime(testnet, preferredApiVersion);

    // Sync every 5 minutes
    if (this.syncInterval) {
      clearInterval(this.syncInterval);
    }
    this.syncInterval = setInterval(() => this.syncClientServerTime(testnet, preferredApiVersion), 5 * 60 * 1000);
  };

  /**
   * Stop the virtual clock and clear sync interval
   */
  stop = () => {
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
  ) => {
    if (typeof window === 'undefined') {
      // Running on server - assume better time sync
      this.server = null;
      this.client = null;
      return;
    }

    this.preferredApiVersion = preferredApiVersion;
    this.server = await this.fetchServerTime(testnet, preferredApiVersion);
    this.client = dayjs.utc();

    // Always apply the offset, even if small
    // This ensures we're always in sync with the server
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
  ): Promise<Dayjs> => {
    const baseUrl = testnet ? 'https://testnet-fapi.asterdex.com' : 'https://fapi.asterdex.com';
    const timePathCandidates = this.getTimePathCandidates(preferredApiVersion);

    for (const timePath of timePathCandidates) {
      try {
        const { data } = await axios.get(`${baseUrl}${timePath}`, { timeout: 5000 });
        if (data.serverTime) {
          return dayjs(data.serverTime);
        }
      } catch {
        // Ignore error and continue to next candidate
      }
    }

    // Try through proxy
    for (const timePath of timePathCandidates) {
      try {
        const { data } = await axios.get(`${getPublicCorsProxy()}/${baseUrl}${timePath}`, { timeout: 5000 });
        if (data.serverTime) {
          return dayjs(data.serverTime);
        }
      } catch {
        // Ignore error and continue to next candidate
      }
    }

    // Fallback to local time
    return dayjs.utc();
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
