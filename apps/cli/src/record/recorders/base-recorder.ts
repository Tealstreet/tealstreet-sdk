/**
 * Base Recorder
 *
 * Abstract base class for all data recorders.
 * Provides common functionality for interval checking, cleanup scheduling, and status tracking.
 */

import type { DataType, RecorderStatus, RecordingOptions } from '../types';
import { cleanupOldData, upsertRecording, deactivateRecording, getLastOrderbookTimestamp } from '../database';
import { logger } from '../../logger';

/**
 * Abstract base class for data recorders
 */
export abstract class BaseRecorder {
  abstract readonly dataType: DataType;

  protected exchange: string;
  protected symbol: string;
  protected intervalMs: number;
  protected retentionDays: number;
  protected levels: number;

  protected lastRecordTime = 0;
  protected snapshotCount = 0;
  protected cleanupInterval?: NodeJS.Timeout;
  protected running = false;

  constructor(options: RecordingOptions) {
    this.exchange = options.exchange;
    this.symbol = options.symbol;
    this.intervalMs = options.intervalMs;
    this.retentionDays = options.retentionDays;
    this.levels = options.levels ?? 25;
  }

  /**
   * Start recording
   */
  abstract start(): Promise<void>;

  /**
   * Stop recording
   */
  abstract stop(): Promise<void>;

  /**
   * Check if recording is running
   */
  isRunning(): boolean {
    return this.running;
  }

  /**
   * Get recorder status
   */
  getStatus(): RecorderStatus {
    return {
      dataType: this.dataType,
      exchange: this.exchange,
      symbol: this.symbol,
      active: this.running,
      lastSnapshotTime: this.lastRecordTime || null,
      snapshotCount: this.snapshotCount,
      intervalMs: this.intervalMs,
      retentionDays: this.retentionDays,
    };
  }

  /**
   * Check if enough time has elapsed to record another snapshot
   */
  protected shouldRecord(): boolean {
    const now = Date.now();
    if (now - this.lastRecordTime >= this.intervalMs) {
      this.lastRecordTime = now;
      return true;
    }
    return false;
  }

  /**
   * Schedule periodic cleanup of old data
   */
  protected scheduleCleanup(): void {
    // Run cleanup every hour
    this.cleanupInterval = setInterval(() => {
      this.runCleanup();
    }, 60 * 60 * 1000);

    // Also run cleanup immediately on start
    this.runCleanup();
  }

  /**
   * Stop the cleanup scheduler
   */
  protected stopCleanup(): void {
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
      this.cleanupInterval = undefined;
    }
  }

  /**
   * Run cleanup to delete old data
   */
  protected runCleanup(): void {
    const retentionMs = this.retentionDays * 24 * 60 * 60 * 1000;
    const cutoff = Date.now() - retentionMs;
    const deleted = cleanupOldData(this.dataType, cutoff);
    if (deleted > 0) {
      logger.info(`[${this.dataType}] Cleaned up ${deleted} old snapshots`);
    }
  }

  /**
   * Save recording configuration to database
   */
  protected saveConfig(active: boolean): void {
    upsertRecording({
      dataType: this.dataType,
      exchange: this.exchange,
      symbol: this.symbol,
      intervalMs: this.intervalMs,
      retentionDays: this.retentionDays,
      config: { levels: this.levels },
      active,
    });
  }

  /**
   * Mark recording as inactive in database
   */
  protected deactivateConfig(): void {
    deactivateRecording(this.dataType, this.exchange, this.symbol);
  }

  /**
   * Log a message with recorder context
   */
  protected log(message: string): void {
    logger.info(`[${this.dataType}:${this.exchange}:${this.symbol}] ${message}`);
  }

  /**
   * Log an error with recorder context
   */
  protected logError(message: string, error?: Error): void {
    logger.error(`[${this.dataType}:${this.exchange}:${this.symbol}] ${message}`, error?.stack);
  }
}
