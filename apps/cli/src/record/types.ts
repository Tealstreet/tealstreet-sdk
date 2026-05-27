/**
 * Market Data Recording Types
 *
 * Defines types for the market data recording system.
 * Designed to be extensible for multiple data types (orderbook, trades, funding, etc.)
 */

/**
 * Supported data types for recording
 */
export type DataType = 'orderbook' | 'trades' | 'funding';

/**
 * Recording configuration stored in the database
 */
export interface RecordingConfig {
  id?: number;
  dataType: DataType;
  exchange: string;
  symbol: string;
  intervalMs: number;
  retentionDays: number;
  config: TypeSpecificConfig | null;
  active: boolean;
  createdAt: number;
}

/**
 * Type-specific configuration (stored as JSON)
 */
export interface TypeSpecificConfig {
  // Orderbook-specific
  levels?: number;
  // Future: trades-specific, funding-specific, etc.
}

/**
 * Orderbook snapshot stored in the database
 */
export interface OrderbookSnapshot {
  id?: number;
  exchange: string;
  symbol: string;
  timestamp: number;
  bids: [number, number][]; // [[price, amount], ...]
  asks: [number, number][]; // [[price, amount], ...]
  createdAt?: number;
}

/**
 * Recording status returned by recorder
 */
export interface RecorderStatus {
  dataType: DataType;
  exchange: string;
  symbol: string;
  active: boolean;
  lastSnapshotTime: number | null;
  snapshotCount: number;
  intervalMs: number;
  retentionDays: number;
}

/**
 * Options for starting a recording
 */
export interface RecordingOptions {
  dataType: DataType;
  exchange: string;
  symbol: string;
  intervalMs: number;
  retentionDays: number;
  levels?: number; // For orderbook
}

/**
 * Query parameters for fetching historical data
 */
export interface HistoryQuery {
  dataType: DataType;
  exchange: string;
  symbol: string;
  from: number;
  to: number;
  limit?: number;
  offset?: number;
}

/**
 * Response for history API
 */
export interface HistoryResponse<T> {
  type: DataType;
  exchange: string;
  symbol: string;
  snapshots: T[];
  count: number;
  hasMore: boolean;
}

/**
 * Database statistics
 */
export interface DatabaseStats {
  totalSnapshots: number;
  databaseSize: string;
  byType: Record<string, { count: number; exchanges: string[] }>;
}

/**
 * Parse interval string to milliseconds
 * Supports: 30s, 1m, 5m, 1h, etc.
 */
export function parseInterval(interval: string): number {
  const match = interval.match(/^(\d+)(s|m|h|d)$/);
  if (!match) {
    throw new Error(`Invalid interval format: ${interval}. Use format like 30s, 1m, 5m, 1h`);
  }

  const value = parseInt(match[1], 10);
  const unit = match[2];

  switch (unit) {
    case 's':
      return value * 1000;
    case 'm':
      return value * 60 * 1000;
    case 'h':
      return value * 60 * 60 * 1000;
    case 'd':
      return value * 24 * 60 * 60 * 1000;
    default:
      throw new Error(`Unknown interval unit: ${unit}`);
  }
}

/**
 * Parse retention string to days
 * Supports: 1d, 7d, 14d, 30d, etc.
 */
export function parseRetention(retention: string): number {
  const match = retention.match(/^(\d+)d$/);
  if (!match) {
    throw new Error(`Invalid retention format: ${retention}. Use format like 7d, 14d, 30d`);
  }
  return parseInt(match[1], 10);
}

/**
 * Format bytes to human readable string
 */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}
