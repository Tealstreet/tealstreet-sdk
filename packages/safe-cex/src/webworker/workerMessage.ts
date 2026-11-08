import type { StoreWorkerIncomingOp, StoreWorkerPayload } from './StoreWorkerPayload';
import type { WriteMarks } from '../writeTiming';
import { WRITE_STAGES } from '../writeTiming';

const incomingOps = {
  'init': true,
  'dispose': true,
  'reinit': true,
  'deactivate': true,
  'sayHi': true,
  'forwardFrames': true,
  'store': true,
  'merge-loaded': true,
  'merge-message-handler-stats': true,
  'sendSnapshot': true,
  'settings': true,
  'sync-time': true,
  'subscribeOHLCV': true,
  'unsubscribeOHLCV': true,
  'subscribeOrderBook': true,
  'unsubscribeOrderBook': true,
  'chaser:start': true,
  'chaser:terminate': true,
  'chaser:terminateAll': true,
  'twap:start': true,
  'twap:resume': true,
  'twap:cleanup': true,
  'twap:pause': true,
  'twap:stop': true,
  'twap:state': true,
  'updatePositions': true,
  'callMethod': true,
  'exec': true,
  'dev:simulateWebSocketsReconnect': true,
  'dev:simulateHOWClose': true,
  'dev:forceOpen': true,
  'dev:forceClose': true,
  'ui:state': true,
  'sign-res': true,
  '__worker_ping__': true,
  '__workerConfig': true,
  '__perfFixtureDump': true,
} satisfies Record<StoreWorkerIncomingOp, true>;

export const isWorkerRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

export type WorkerMessage =
  | { op: 'callMethod'; instanceId?: string; data?: unknown }
  | (Omit<StoreWorkerPayload, 'op'> & { op: Exclude<StoreWorkerIncomingOp, 'callMethod'> });

/** Check routing fields only; each operation retains its existing domain validation. */
export function isWorkerMessage(value: unknown): value is WorkerMessage {
  if (!isWorkerRecord(value) || typeof value.op !== 'string' || !Object.hasOwn(incomingOps, value.op)) return false;
  if (value.instanceId !== undefined && typeof value.instanceId !== 'string') return false;
  if (value.op === '__workerConfig' && value.data !== undefined && !isWorkerRecord(value.data)) return false;
  if (value.op === 'init') {
    if (!isWorkerRecord(value.data) || typeof value.data.exchangeName !== 'string') return false;
    if (value.data.accountName !== undefined && typeof value.data.accountName !== 'string') return false;
    if (!isWorkerRecord(value.data.options)) return false;
    for (const field of ['settings', 'metadata', 'store']) {
      if (value.data[field] !== undefined && !isWorkerRecord(value.data[field])) return false;
    }
  }
  return true;
}

export const isWorkerWriteMarks = (value: unknown): value is WriteMarks =>
  isWorkerRecord(value) && WRITE_STAGES.every(stage => value[stage] === undefined || typeof value[stage] === 'number');
