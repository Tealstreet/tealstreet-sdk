/**
 * Recorder Factory
 *
 * Creates recorder instances based on data type.
 * Manages active recorders and provides lookup functionality.
 */

import type { DataType, RecordingOptions } from '../types';
import { BaseRecorder } from './base-recorder';
import { OrderbookRecorder } from './orderbook';

// Active recorders keyed by "dataType:exchange:symbol"
const activeRecorders = new Map<string, BaseRecorder>();

/**
 * Generate a unique key for a recorder
 */
function getRecorderKey(dataType: DataType, exchange: string, symbol: string): string {
  return `${dataType}:${exchange}:${symbol}`;
}

/**
 * Create a recorder instance based on data type
 */
export function createRecorder(options: RecordingOptions): BaseRecorder {
  switch (options.dataType) {
    case 'orderbook':
      return new OrderbookRecorder(options);
    // Future: add cases for 'trades', 'funding', etc.
    default:
      throw new Error(`Unknown data type: ${options.dataType}`);
  }
}

/**
 * Start a recorder and add it to active recorders.
 * If a recorder with the same key is already running, it will be stopped and replaced.
 * Returns { recorder, replaced: boolean }
 */
export async function startRecorder(
  options: RecordingOptions
): Promise<{ recorder: BaseRecorder; replaced: boolean }> {
  const key = getRecorderKey(options.dataType, options.exchange, options.symbol);
  let replaced = false;

  // Stop existing recorder if running (replace behavior)
  const existing = activeRecorders.get(key);
  if (existing && existing.isRunning()) {
    await existing.stop();
    activeRecorders.delete(key);
    replaced = true;
  }

  // Create and start new recorder
  const recorder = createRecorder(options);
  await recorder.start();

  // Add to active recorders
  activeRecorders.set(key, recorder);

  return { recorder, replaced };
}

/**
 * Stop a specific recorder
 */
export async function stopRecorder(dataType: DataType, exchange: string, symbol: string): Promise<boolean> {
  const key = getRecorderKey(dataType, exchange, symbol);
  const recorder = activeRecorders.get(key);

  if (!recorder) {
    return false;
  }

  await recorder.stop();
  activeRecorders.delete(key);
  return true;
}

/**
 * Stop all recorders for an exchange
 */
export async function stopExchangeRecorders(exchange: string): Promise<number> {
  let stopped = 0;

  for (const [key, recorder] of activeRecorders.entries()) {
    if (key.includes(`:${exchange}:`)) {
      await recorder.stop();
      activeRecorders.delete(key);
      stopped++;
    }
  }

  return stopped;
}

/**
 * Stop all active recorders
 */
export async function stopAllRecorders(): Promise<number> {
  let stopped = 0;

  for (const [key, recorder] of activeRecorders.entries()) {
    await recorder.stop();
    activeRecorders.delete(key);
    stopped++;
  }

  return stopped;
}

/**
 * Get a specific recorder
 */
export function getRecorder(dataType: DataType, exchange: string, symbol: string): BaseRecorder | undefined {
  const key = getRecorderKey(dataType, exchange, symbol);
  return activeRecorders.get(key);
}

/**
 * Get all active recorders
 */
export function getActiveRecorders(): BaseRecorder[] {
  return Array.from(activeRecorders.values());
}

/**
 * Check if a recorder is running
 */
export function isRecorderRunning(dataType: DataType, exchange: string, symbol: string): boolean {
  const recorder = getRecorder(dataType, exchange, symbol);
  return recorder?.isRunning() ?? false;
}

// Re-export types and classes
export { BaseRecorder } from './base-recorder';
export { OrderbookRecorder } from './orderbook';
