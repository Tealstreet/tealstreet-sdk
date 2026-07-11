import axios from 'axios';

import { compress } from '@tealstreet/compression/src/compress';
import { StoreData, Writable } from '../../../types';
import { BaseTaskPostprocessor } from './BaseTaskPostprocessor';

const SNAPSHOT_INTERVAL_MS = 5 * 60_000;
const SNAPSHOT_INGEST_TOKEN_REFRESH_SKEW_MS = 60_000;

type SnapshotIngestToken = {
  expiresAt: number;
  token: string;
};

let cachedSnapshotIngestToken: SnapshotIngestToken | undefined;
let snapshotIngestTokenRequest: Promise<SnapshotIngestToken> | undefined;

const getSnapshotIngestOrigin = () => {
  const origin = typeof process !== 'undefined' ? process.env?.NEXT_PUBLIC_SNAPSHOT_INGEST_ORIGIN : undefined;
  return origin?.replace(/\/+$/, '') || undefined;
};

const getSnapshotIngestToken = async (baseUrl: string) => {
  if (
    cachedSnapshotIngestToken &&
    cachedSnapshotIngestToken.expiresAt - Date.now() > SNAPSHOT_INGEST_TOKEN_REFRESH_SKEW_MS
  ) {
    return cachedSnapshotIngestToken.token;
  }

  if (!snapshotIngestTokenRequest) {
    snapshotIngestTokenRequest = fetch(`${baseUrl}/api/snapshot/token`, { credentials: 'same-origin' })
      .then(async (response) => {
        if (!response.ok) throw new Error(`Snapshot token API responded with status ${response.status}`);
        const payload = (await response.json()) as Partial<SnapshotIngestToken>;
        if (typeof payload.token !== 'string' || typeof payload.expiresAt !== 'number') {
          throw new Error('Snapshot token API returned an invalid response');
        }
        cachedSnapshotIngestToken = { expiresAt: payload.expiresAt, token: payload.token };
        return cachedSnapshotIngestToken;
      })
      .finally(() => {
        snapshotIngestTokenRequest = undefined;
      });
  }

  return (await snapshotIngestTokenRequest).token;
};

export class SnapshotPostprocessor extends BaseTaskPostprocessor {
  private _lastUpdate: number = Date.now() - 60_000;
  private _lastLongUpdate: number = 0;
  private _initializationTime: number = Date.now();
  private _previousHashes: {
    equity_curve?: string;
    order_history?: string;
    position_history?: string;
    userdata?: string;
  } = {};
  // Store base URL at construction time - more reliable than checking self.location each time
  private _baseUrl: string | null = this._resolveBaseUrl();

  private _resolveBaseUrl(): string | null {
    // Skip in Node.js CLI context - check for browser/worker-specific global
    if (typeof XMLHttpRequest === 'undefined') {
      return null;
    }
    // Try to get origin from self.location (works in web workers)
    if (typeof self !== 'undefined' && self.location?.origin && self.location.origin !== 'null') {
      return self.location.origin;
    }
    return null;
  }

  async process(store: Writable<StoreData>): Promise<void> {
    if (process.env.NEXT_PUBLIC_DISABLE_EXTRA_APIS_IN_DEV === '1') {
      return;
    }

    // Skip if no valid base URL (CLI mode or blob URL without origin)
    if (!this._baseUrl) {
      return;
    }

    if (this._accountName === undefined) {
      return;
    }

    // Skip snapshots for the first 20 seconds
    if (Date.now() - this._initializationTime < 20_000) {
      return;
    }

    // Skip snapshots if exchange is not started (essential data not loaded)
    if (!store.loaded?.markets || !store.loaded?.tickers) {
      return;
    }

    if (this._lastUpdate === 0 || Date.now() - this._lastUpdate > SNAPSHOT_INTERVAL_MS) {
      this._lastUpdate = Date.now();

      // Prepare snapshot data object
      const snapshotData: any = {};

      // Add balance data (equity_curve) - only if changed
      if (store.balance) {
        const balanceHash = this.createHash(store.balance);
        if (balanceHash !== this._previousHashes.equity_curve) {
          snapshotData.equity_curve = store.balance;
          this._previousHashes.equity_curve = balanceHash;
        }
      }

      // Add orders data (order_history) - only if changed
      if (store.orders) {
        const ordersHash = this.createHash(store.orders);
        if (ordersHash !== this._previousHashes.order_history) {
          snapshotData.order_history = store.orders;
          this._previousHashes.order_history = ordersHash;
        }
      }

      // Add positions data (position_history) - only if changed
      const positions = store.displayedPositions ?? store.positions;
      if (positions) {
        // Ensure all positions have non-null contracts
        const positionsWithContracts = positions.map((position) => ({
          ...position,
          contracts: position.contracts ?? 0,
        }));

        const positionsHash = this.createHash(positionsWithContracts);
        if (positionsHash !== this._previousHashes.position_history) {
          snapshotData.position_history = positionsWithContracts;
          this._previousHashes.position_history = positionsHash;
        }
      }

      // Add userdata only once per hour and if changed
      if (store.userData) {
        if (this._lastLongUpdate === 0 || Date.now() - this._lastLongUpdate > 3_600_000) {
          const userDataHash = this.createHash(store.userData);
          if (userDataHash !== this._previousHashes.userdata) {
            this._lastLongUpdate = Date.now();
            snapshotData.userdata = store.userData;
            this._previousHashes.userdata = userDataHash;
          }
        }
      }

      // Send all data in a single request to snapshot endpoint
      if (Object.keys(snapshotData).length > 0) {
        const query = new URLSearchParams({ api_key_name: this._accountName, exchange: this._exchangeName });
        const snapshotIngestOrigin = getSnapshotIngestOrigin();
        const url = snapshotIngestOrigin
          ? `${snapshotIngestOrigin}/v1/snapshots?${query}`
          : `${this._baseUrl}/api/snapshot?${query}`;
        try {
          const compressedData = await compress(this.safeStringify(snapshotData));
          const headers = snapshotIngestOrigin
            ? { Authorization: `Bearer ${await getSnapshotIngestToken(this._baseUrl)}` }
            : undefined;
          await axios.post(url, compressedData, { headers });
        } catch (error) {
          // Log error but don't throw - snapshots are non-critical
          console.error('[SnapshotPostprocessor] Failed to send snapshot:', error);
        }
      }
    }
  }

  private safeStringify(data: any): string {
    // JSON.stringify with circular reference handling
    const seen = new WeakSet();
    return JSON.stringify(data, (key, value) => {
      // Skip 'info' property as it contains raw exchange data that may have circular refs
      if (key === 'info') return undefined;
      // Handle other circular references
      if (typeof value === 'object' && value !== null) {
        if (seen.has(value)) return '[Circular]';
        seen.add(value);
      }
      return value;
    });
  }

  private createHash(data: any): string {
    // Simple hash function for comparing data
    return this.safeStringify(data);
  }
}
