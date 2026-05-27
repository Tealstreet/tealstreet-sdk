/**
 * Market Data Database
 *
 * SQLite wrapper for storing and querying market data snapshots.
 * Uses Bun's built-in SQLite for zero-dependency operation.
 */

import { Database } from 'bun:sqlite';
import { join } from 'path';
import { existsSync, mkdirSync, statSync } from 'fs';
import { getConfigDir } from '../config';
import { runMigrations } from './migrations';
import type {
  DataType,
  RecordingConfig,
  OrderbookSnapshot,
  HistoryQuery,
  DatabaseStats,
  TypeSpecificConfig,
  formatBytes,
} from './types';
import { formatBytes as formatBytesUtil } from './types';

const DB_FILENAME = 'market-data.db';

let db: Database | null = null;

/**
 * Get the database file path
 */
export function getDatabasePath(): string {
  return join(getConfigDir(), DB_FILENAME);
}

/**
 * Initialize database connection and run migrations
 */
export function initDatabase(): Database {
  if (db) {
    return db;
  }

  const configDir = getConfigDir();
  if (!existsSync(configDir)) {
    mkdirSync(configDir, { recursive: true });
  }

  const dbPath = getDatabasePath();
  db = new Database(dbPath);

  // Enable WAL mode for better concurrent access
  db.exec('PRAGMA journal_mode = WAL');

  // Run migrations
  runMigrations(db);

  return db;
}

/**
 * Get the database instance (initializes if needed)
 */
export function getDatabase(): Database {
  if (!db) {
    return initDatabase();
  }
  return db;
}

/**
 * Close the database connection
 */
export function closeDatabase(): void {
  if (db) {
    db.close();
    db = null;
  }
}

// ============================================================================
// Recording Configuration
// ============================================================================

/**
 * Get all recording configurations
 */
export function getRecordings(): RecordingConfig[] {
  const database = getDatabase();
  const rows = database
    .query(
      `SELECT id, data_type, exchange, symbol, interval_ms, retention_days, config, active, created_at
       FROM recording_config
       ORDER BY created_at DESC`
    )
    .all() as {
    id: number;
    data_type: string;
    exchange: string;
    symbol: string;
    interval_ms: number;
    retention_days: number;
    config: string | null;
    active: number;
    created_at: number;
  }[];

  return rows.map((row) => ({
    id: row.id,
    dataType: row.data_type as DataType,
    exchange: row.exchange,
    symbol: row.symbol,
    intervalMs: row.interval_ms,
    retentionDays: row.retention_days,
    config: row.config ? JSON.parse(row.config) : null,
    active: row.active === 1,
    createdAt: row.created_at,
  }));
}

/**
 * Get active recordings (for auto-start)
 */
export function getActiveRecordings(): RecordingConfig[] {
  return getRecordings().filter((r) => r.active);
}

/**
 * Get a specific recording configuration
 */
export function getRecording(dataType: DataType, exchange: string, symbol: string): RecordingConfig | null {
  const database = getDatabase();
  const row = database
    .query(
      `SELECT id, data_type, exchange, symbol, interval_ms, retention_days, config, active, created_at
       FROM recording_config
       WHERE data_type = ? AND exchange = ? AND symbol = ?`
    )
    .get(dataType, exchange, symbol) as {
    id: number;
    data_type: string;
    exchange: string;
    symbol: string;
    interval_ms: number;
    retention_days: number;
    config: string | null;
    active: number;
    created_at: number;
  } | null;

  if (!row) return null;

  return {
    id: row.id,
    dataType: row.data_type as DataType,
    exchange: row.exchange,
    symbol: row.symbol,
    intervalMs: row.interval_ms,
    retentionDays: row.retention_days,
    config: row.config ? JSON.parse(row.config) : null,
    active: row.active === 1,
    createdAt: row.created_at,
  };
}

/**
 * Add or update a recording configuration
 */
export function upsertRecording(config: Omit<RecordingConfig, 'id' | 'createdAt'>): void {
  const database = getDatabase();
  const configJson = config.config ? JSON.stringify(config.config) : null;

  database
    .query(
      `INSERT INTO recording_config (data_type, exchange, symbol, interval_ms, retention_days, config, active)
       VALUES (?, ?, ?, ?, ?, ?, ?)
       ON CONFLICT(data_type, exchange, symbol) DO UPDATE SET
         interval_ms = excluded.interval_ms,
         retention_days = excluded.retention_days,
         config = excluded.config,
         active = excluded.active`
    )
    .run(
      config.dataType,
      config.exchange,
      config.symbol,
      config.intervalMs,
      config.retentionDays,
      configJson,
      config.active ? 1 : 0
    );
}

/**
 * Deactivate a recording
 */
export function deactivateRecording(dataType: DataType, exchange: string, symbol: string): void {
  const database = getDatabase();
  database
    .query(`UPDATE recording_config SET active = 0 WHERE data_type = ? AND exchange = ? AND symbol = ?`)
    .run(dataType, exchange, symbol);
}

/**
 * Delete a recording configuration
 */
export function deleteRecording(dataType: DataType, exchange: string, symbol: string): void {
  const database = getDatabase();
  database
    .query(`DELETE FROM recording_config WHERE data_type = ? AND exchange = ? AND symbol = ?`)
    .run(dataType, exchange, symbol);
}

// ============================================================================
// Orderbook Snapshots
// ============================================================================

/**
 * Insert an orderbook snapshot
 */
export function insertOrderbookSnapshot(
  exchange: string,
  symbol: string,
  timestamp: number,
  bids: [number, number][],
  asks: [number, number][]
): void {
  const database = getDatabase();
  database
    .query(
      `INSERT INTO orderbook_snapshots (exchange, symbol, timestamp, bids, asks)
       VALUES (?, ?, ?, ?, ?)`
    )
    .run(exchange, symbol, timestamp, JSON.stringify(bids), JSON.stringify(asks));
}

/**
 * Query orderbook snapshots
 */
export function queryOrderbookSnapshots(query: HistoryQuery): OrderbookSnapshot[] {
  const database = getDatabase();
  const limit = query.limit ?? 1000;
  const offset = query.offset ?? 0;

  const rows = database
    .query(
      `SELECT id, exchange, symbol, timestamp, bids, asks, created_at
       FROM orderbook_snapshots
       WHERE exchange = ? AND symbol = ? AND timestamp >= ? AND timestamp <= ?
       ORDER BY timestamp ASC
       LIMIT ? OFFSET ?`
    )
    .all(query.exchange, query.symbol, query.from, query.to, limit, offset) as {
    id: number;
    exchange: string;
    symbol: string;
    timestamp: number;
    bids: string;
    asks: string;
    created_at: number;
  }[];

  return rows.map((row) => ({
    id: row.id,
    exchange: row.exchange,
    symbol: row.symbol,
    timestamp: row.timestamp,
    bids: JSON.parse(row.bids),
    asks: JSON.parse(row.asks),
    createdAt: row.created_at,
  }));
}

/**
 * Count orderbook snapshots matching query
 */
export function countOrderbookSnapshots(exchange: string, symbol: string, from?: number, to?: number): number {
  const database = getDatabase();

  if (from !== undefined && to !== undefined) {
    const result = database
      .query(
        `SELECT COUNT(*) as count FROM orderbook_snapshots
         WHERE exchange = ? AND symbol = ? AND timestamp >= ? AND timestamp <= ?`
      )
      .get(exchange, symbol, from, to) as { count: number };
    return result.count;
  }

  const result = database
    .query(`SELECT COUNT(*) as count FROM orderbook_snapshots WHERE exchange = ? AND symbol = ?`)
    .get(exchange, symbol) as { count: number };
  return result.count;
}

/**
 * Get the most recent orderbook snapshot timestamp
 */
export function getLastOrderbookTimestamp(exchange: string, symbol: string): number | null {
  const database = getDatabase();
  const result = database
    .query(
      `SELECT MAX(timestamp) as timestamp FROM orderbook_snapshots
       WHERE exchange = ? AND symbol = ?`
    )
    .get(exchange, symbol) as { timestamp: number | null };
  return result.timestamp;
}

// ============================================================================
// Cleanup
// ============================================================================

/**
 * Delete old snapshots based on retention period
 */
export function cleanupOldData(dataType: DataType, cutoffTimestamp: number): number {
  const database = getDatabase();

  switch (dataType) {
    case 'orderbook': {
      const result = database
        .query(`DELETE FROM orderbook_snapshots WHERE timestamp < ?`)
        .run(cutoffTimestamp);
      return result.changes;
    }
    // Future: add cases for 'trades', 'funding', etc.
    default:
      return 0;
  }
}

/**
 * Delete all data for a specific recording
 */
export function deleteAllData(dataType: DataType, exchange: string, symbol: string): number {
  const database = getDatabase();

  switch (dataType) {
    case 'orderbook': {
      const result = database
        .query(`DELETE FROM orderbook_snapshots WHERE exchange = ? AND symbol = ?`)
        .run(exchange, symbol);
      return result.changes;
    }
    default:
      return 0;
  }
}

// ============================================================================
// Statistics
// ============================================================================

/**
 * Get database statistics
 */
export function getDatabaseStats(): DatabaseStats {
  const database = getDatabase();
  const dbPath = getDatabasePath();

  // Get database file size
  let databaseSize = '0 B';
  if (existsSync(dbPath)) {
    const stats = statSync(dbPath);
    databaseSize = formatBytesUtil(stats.size);
  }

  // Get orderbook stats
  const orderbookCount = database
    .query(`SELECT COUNT(*) as count FROM orderbook_snapshots`)
    .get() as { count: number };

  const orderbookExchanges = database
    .query(`SELECT DISTINCT exchange FROM orderbook_snapshots`)
    .all() as { exchange: string }[];

  return {
    totalSnapshots: orderbookCount.count,
    databaseSize,
    byType: {
      orderbook: {
        count: orderbookCount.count,
        exchanges: orderbookExchanges.map((r) => r.exchange),
      },
    },
  };
}
