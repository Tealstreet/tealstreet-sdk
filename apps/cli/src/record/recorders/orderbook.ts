/**
 * Orderbook Recorder
 *
 * Records orderbook snapshots at configurable intervals.
 * Uses public WebSocket connection (no API credentials required).
 */

import { createExchange } from '../../../../../packages/safe-cex/src/index';
import type { BaseExchange } from '../../../../../packages/safe-cex/src/exchanges/base';
import type { ExchangeName, OrderBook } from '../../../../../packages/safe-cex/src/types';
import { BaseRecorder } from './base-recorder';
import type { DataType, RecordingOptions } from '../types';
import { insertOrderbookSnapshot, countOrderbookSnapshots } from '../database';

/**
 * Orderbook recorder implementation
 */
export class OrderbookRecorder extends BaseRecorder {
  readonly dataType: DataType = 'orderbook';

  private exchange_instance: BaseExchange | null = null;
  private unsubscribe: (() => void) | null = null;

  constructor(options: RecordingOptions) {
    super(options);
  }

  /**
   * Start recording orderbook snapshots
   */
  async start(): Promise<void> {
    if (this.running) {
      this.log('Already running');
      return;
    }

    this.log(`Starting recorder (interval: ${this.intervalMs}ms, retention: ${this.retentionDays}d, levels: ${this.levels})`);

    try {
      // Create exchange in public-only mode (no API credentials needed)
      this.exchange_instance = await createExchange(this.exchange as ExchangeName, {
        key: '',
        secret: '',
        publicDataOnly: true,
        testnet: false,
      });

      // Start the exchange (connects public WebSocket)
      await this.exchange_instance.start();

      // Subscribe to orderbook updates
      this.unsubscribe = this.exchange_instance.listenOrderBook(this.symbol, (orderbook: OrderBook) => {
        this.onOrderbookUpdate(orderbook);
      });

      this.running = true;

      // Save config to database
      this.saveConfig(true);

      // Schedule cleanup
      this.scheduleCleanup();

      // Get initial snapshot count
      this.snapshotCount = countOrderbookSnapshots(this.exchange, this.symbol);

      this.log('Recorder started successfully');
    } catch (error) {
      this.logError('Failed to start recorder', error as Error);
      await this.stop();
      throw error;
    }
  }

  /**
   * Stop recording
   */
  async stop(): Promise<void> {
    this.log('Stopping recorder...');

    // Stop cleanup scheduler
    this.stopCleanup();

    // Unsubscribe from orderbook updates
    if (this.unsubscribe) {
      this.unsubscribe();
      this.unsubscribe = null;
    }

    // Dispose the exchange
    if (this.exchange_instance) {
      try {
        this.exchange_instance.dispose();
      } catch (error) {
        this.logError('Error disposing exchange', error as Error);
      }
      this.exchange_instance = null;
    }

    this.running = false;

    // Mark as inactive in database
    this.deactivateConfig();

    this.log('Recorder stopped');
  }

  /**
   * Handle orderbook update from exchange
   */
  private onOrderbookUpdate(orderbook: OrderBook): void {
    if (!this.running) return;

    // Check if enough time has elapsed
    if (!this.shouldRecord()) {
      return;
    }

    try {
      // Trim orderbook to configured number of levels
      const bids = this.trimLevels(orderbook.bids);
      const asks = this.trimLevels(orderbook.asks);

      // Insert snapshot
      const timestamp = Date.now();
      insertOrderbookSnapshot(this.exchange, this.symbol, timestamp, bids, asks);

      this.snapshotCount++;

      // Log periodically (every 10 snapshots)
      if (this.snapshotCount % 10 === 0) {
        this.log(`Recorded ${this.snapshotCount} snapshots`);
      }
    } catch (error) {
      this.logError('Failed to record snapshot', error as Error);
    }
  }

  /**
   * Trim orderbook to configured number of levels
   */
  private trimLevels(levels: OrderBook['bids'] | OrderBook['asks']): [number, number][] {
    return levels.slice(0, this.levels).map((level) => [level.price, level.amount]);
  }
}
