/**
 * Database Schema Migrations
 *
 * Manages schema versioning and migrations for the market data database.
 * Each migration has a version number, name, and SQL to apply.
 */

import type { Database } from 'bun:sqlite';

/**
 * Migration definition
 */
export interface Migration {
  version: number;
  name: string;
  up: string;
}

/**
 * All migrations in order
 */
export const migrations: Migration[] = [
  {
    version: 1,
    name: 'initial_schema',
    up: `
      -- Schema version tracking
      CREATE TABLE IF NOT EXISTS schema_version (
        version INTEGER PRIMARY KEY
      );

      -- Generic recording configuration (supports multiple data types)
      CREATE TABLE IF NOT EXISTS recording_config (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        data_type TEXT NOT NULL,
        exchange TEXT NOT NULL,
        symbol TEXT NOT NULL,
        interval_ms INTEGER NOT NULL DEFAULT 60000,
        retention_days INTEGER NOT NULL DEFAULT 7,
        config TEXT,
        active INTEGER NOT NULL DEFAULT 1,
        created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000),
        UNIQUE(data_type, exchange, symbol)
      );

      -- Orderbook snapshots
      CREATE TABLE IF NOT EXISTS orderbook_snapshots (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        exchange TEXT NOT NULL,
        symbol TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        bids TEXT NOT NULL,
        asks TEXT NOT NULL,
        created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
      );

      -- Index for efficient time-range queries
      CREATE INDEX IF NOT EXISTS idx_orderbook_lookup
      ON orderbook_snapshots(exchange, symbol, timestamp);

      -- Index for cleanup queries
      CREATE INDEX IF NOT EXISTS idx_orderbook_timestamp
      ON orderbook_snapshots(timestamp);

      -- Insert initial version
      INSERT INTO schema_version (version) VALUES (1);
    `,
  },
  // Future migrations will be added here:
  // {
  //   version: 2,
  //   name: 'add_trades_table',
  //   up: `
  //     CREATE TABLE IF NOT EXISTS trades_snapshots (...);
  //     UPDATE schema_version SET version = 2;
  //   `,
  // },
];

/**
 * Get current schema version from database
 */
export function getCurrentVersion(db: Database): number {
  try {
    const result = db.query('SELECT version FROM schema_version LIMIT 1').get() as
      | { version: number }
      | null;
    return result?.version ?? 0;
  } catch {
    // Table doesn't exist yet
    return 0;
  }
}

/**
 * Run all pending migrations
 */
export function runMigrations(db: Database): void {
  const currentVersion = getCurrentVersion(db);

  for (const migration of migrations) {
    if (migration.version > currentVersion) {
      console.log(`Running migration ${migration.version}: ${migration.name}`);
      db.exec(migration.up);
    }
  }
}

/**
 * Get latest migration version
 */
export function getLatestVersion(): number {
  return migrations.length > 0 ? migrations[migrations.length - 1].version : 0;
}
