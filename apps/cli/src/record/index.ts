/**
 * Market Data Recording Module
 *
 * Provides functionality for recording and querying historical market data.
 * Phase 1: Orderbook snapshots
 * Future: Trades, funding rates, etc.
 */

// Database
export {
  initDatabase,
  getDatabase,
  closeDatabase,
  getDatabasePath,
  // Recording config
  getRecordings,
  getActiveRecordings,
  getRecording,
  upsertRecording,
  deactivateRecording,
  deleteRecording,
  // Orderbook snapshots
  insertOrderbookSnapshot,
  queryOrderbookSnapshots,
  countOrderbookSnapshots,
  getLastOrderbookTimestamp,
  // Cleanup
  cleanupOldData,
  deleteAllData,
  // Stats
  getDatabaseStats,
} from './database';

// Migrations
export { runMigrations, getCurrentVersion, getLatestVersion } from './migrations';

// Types
export type {
  DataType,
  RecordingConfig,
  TypeSpecificConfig,
  OrderbookSnapshot,
  RecorderStatus,
  RecordingOptions,
  HistoryQuery,
  HistoryResponse,
  DatabaseStats,
} from './types';

export { parseInterval, parseRetention, formatBytes } from './types';

// Recorders
export {
  createRecorder,
  startRecorder,
  stopRecorder,
  stopExchangeRecorders,
  stopAllRecorders,
  getRecorder,
  getActiveRecorders,
  isRecorderRunning,
  BaseRecorder,
  OrderbookRecorder,
} from './recorders';
