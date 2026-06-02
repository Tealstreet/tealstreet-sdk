import type { ServerWebSocket } from 'bun';

import {
  loadCrossTradeFile,
  tombstoneCrossTradeConfig,
  upsertCrossTradeConfigRaw,
  type StoredCrossTradeConfig,
} from '../../cross-trade-store';
import { logger } from '../../logger';
import type { WsSessionData } from '../ws';

function send(ws: ServerWebSocket<WsSessionData>, frame: Record<string, unknown>): void {
  try {
    ws.send(JSON.stringify(frame));
  } catch (err) {
    logger.warn(`mutate:cross-trade-config send failed: ${(err as Error).message}`);
  }
}

function err(ws: ServerWebSocket<WsSessionData>, id: string, message: string): void {
  send(ws, { t: 'mutate:err', id, error: message });
}

function ack(ws: ServerWebSocket<WsSessionData>, id: string): void {
  send(ws, { t: 'mutate:ack', id });
}

function effectiveUpdatedAt(c: { updatedAt?: number } | undefined): number {
  return c?.updatedAt ?? 0;
}

export interface MutateCrossTradeResult {
  acked: boolean;
  wrote: boolean;
}

/**
 * Handle a mutate:cross-trade-config frame from a trusted web client.
 *
 * upsert shape: { t, id, op:'upsert', config: StoredCrossTradeConfig }
 * delete shape: { t, id, op:'delete', configId: string, updatedAt: number }
 *
 * LWW: incoming wins iff `updatedAt` is a positive number that strictly
 * exceeds the local row's. Unlike handlers/account.ts (which intentionally
 * lets first-sync rows with `updatedAt=0` write because the web stamps
 * them via useCliAccountSync), cross-trade configs are always stamped at
 * creation time (`buildSingleCopierConfig` writes `Date.now()`), so a
 * frame with `updatedAt=0` is a sender bug — we reject it with a
 * mutate:err to surface the issue. Stale incoming gets a mutate:ack with
 * no write so the client can drop the pending mutation.
 */
export function handleMutateCrossTrade(ws: ServerWebSocket<WsSessionData>, frame: unknown): MutateCrossTradeResult {
  const f = frame as {
    id?: unknown;
    op?: unknown;
    config?: unknown;
    configId?: unknown;
    updatedAt?: unknown;
  };
  const id = typeof f.id === 'string' ? f.id : '';
  if (!id) {
    logger.warn('mutate:cross-trade-config missing id');
    return { acked: false, wrote: false };
  }

  const op = f.op;
  if (op === 'upsert') {
    const incoming = f.config as StoredCrossTradeConfig | undefined;
    if (!incoming || typeof incoming.id !== 'string' || incoming.id.length === 0) {
      err(ws, id, 'mutate:cross-trade-config upsert missing config.id');
      return { acked: true, wrote: false };
    }
    const incomingUpdatedAt = effectiveUpdatedAt(incoming);
    if (incomingUpdatedAt <= 0) {
      err(ws, id, 'mutate:cross-trade-config upsert missing or invalid config.updatedAt');
      return { acked: true, wrote: false };
    }
    const existing = loadCrossTradeFile().configs.find((c) => c.id === incoming.id);
    const existingUpdatedAt = effectiveUpdatedAt(existing);
    if (existingUpdatedAt >= incomingUpdatedAt) {
      logger.info('mutate:cross-trade-config upsert ignored stale', {
        incomingId: incoming.id,
        incomingUpdatedAt,
        existingUpdatedAt,
      });
      ack(ws, id);
      return { acked: true, wrote: false };
    }
    upsertCrossTradeConfigRaw(incoming);
    logger.info('mutate:cross-trade-config upsert wrote', {
      incomingId: incoming.id,
      incomingUpdatedAt,
      existingUpdatedAt,
    });
    ack(ws, id);
    return { acked: true, wrote: true };
  }

  if (op === 'delete') {
    const configId = typeof f.configId === 'string' ? f.configId : '';
    const incomingUpdatedAt = typeof f.updatedAt === 'number' ? f.updatedAt : 0;
    if (!configId) {
      err(ws, id, 'mutate:cross-trade-config delete missing configId');
      return { acked: true, wrote: false };
    }
    if (incomingUpdatedAt <= 0) {
      err(ws, id, 'mutate:cross-trade-config delete missing or invalid updatedAt');
      return { acked: true, wrote: false };
    }
    const existing = loadCrossTradeFile().configs.find((c) => c.id === configId);
    if (existing && effectiveUpdatedAt(existing) >= incomingUpdatedAt) {
      logger.info('mutate:cross-trade-config delete ignored stale', {
        configId,
        incomingUpdatedAt,
        existingUpdatedAt: effectiveUpdatedAt(existing),
      });
      ack(ws, id);
      return { acked: true, wrote: false };
    }
    tombstoneCrossTradeConfig(configId, incomingUpdatedAt);
    logger.info('mutate:cross-trade-config delete wrote', {
      configId,
      incomingUpdatedAt,
      existingUpdatedAt: effectiveUpdatedAt(existing),
    });
    ack(ws, id);
    return { acked: true, wrote: true };
  }

  err(ws, id, `mutate:cross-trade-config unknown op: ${String(op)}`);
  return { acked: true, wrote: false };
}
