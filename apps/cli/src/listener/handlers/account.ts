import type { ServerWebSocket } from 'bun';
import type { ExchangeAccountConfig } from '@tealstreet/cli-core';
import type { WsSessionData } from '../ws';
import { summarizeAccountForLog } from '../../account-log';
import { logger } from '../../logger';
import { isAccountSyncEnabled } from '../config';
import { loadConfig, upsertAccountRaw, tombstoneAccount } from '../../config';

function send(ws: ServerWebSocket<WsSessionData>, frame: Record<string, unknown>): void {
  try {
    ws.send(JSON.stringify(frame));
  } catch (err) {
    logger.warn(`mutate:account send failed: ${(err as Error).message}`);
  }
}

function err(ws: ServerWebSocket<WsSessionData>, id: string, message: string): void {
  send(ws, { t: 'mutate:err', id, error: message });
}

function ack(ws: ServerWebSocket<WsSessionData>, id: string): void {
  send(ws, { t: 'mutate:ack', id });
}

// Preserve CLI-runtime fields when incoming upsert doesn't carry them.
// Web clients may not know about transient runtime markers; merging
// defensively avoids wiping local state on each sync.
function mergeAccount(
  incoming: ExchangeAccountConfig,
  existing: ExchangeAccountConfig | undefined,
): ExchangeAccountConfig {
  if (!existing) return incoming;
  return {
    ...incoming,
    healthStatus: incoming.healthStatus ?? existing.healthStatus,
    lastSymbol: incoming.lastSymbol ?? existing.lastSymbol,
  };
}

function effectiveUpdatedAt(a: ExchangeAccountConfig | undefined): number {
  return a?.updatedAt ?? 0;
}

export interface MutateAccountResult {
  /** Did this mutation actually write to disk? Caller uses this to gate
   *  the broadcast-after-own-write hook. Stale-LWW acks still return true
   *  for `acked` but false for `wrote`. */
  acked: boolean;
  wrote: boolean;
}

export function handleMutateAccount(ws: ServerWebSocket<WsSessionData>, frame: unknown): MutateAccountResult {
  const f = frame as { id?: unknown; op?: unknown; account?: unknown; name?: unknown; updatedAt?: unknown };
  const id = typeof f.id === 'string' ? f.id : '';
  if (!id) {
    logger.warn('mutate:account missing id');
    return { acked: false, wrote: false };
  }

  if (!isAccountSyncEnabled()) {
    logger.info('mutate:account ignored accountSync disabled');
    err(ws, id, 'accountSync disabled');
    return { acked: true, wrote: false };
  }

  const op = f.op;
  if (op === 'upsert') {
    const incoming = f.account as ExchangeAccountConfig | undefined;
    if (!incoming || typeof incoming.name !== 'string' || incoming.name.length === 0) {
      err(ws, id, 'mutate:account upsert missing account.name');
      return { acked: true, wrote: false };
    }
    const incomingUpdatedAt = effectiveUpdatedAt(incoming);
    const existing = loadConfig().accounts.find((a) => a.name === incoming.name);
    const existingUpdatedAt = effectiveUpdatedAt(existing);
    if (existingUpdatedAt >= incomingUpdatedAt && incomingUpdatedAt > 0) {
      // Stale; ack but don't write.
      logger.info('mutate:account upsert ignored stale', {
        incoming: summarizeAccountForLog(incoming),
        existing: existing ? summarizeAccountForLog(existing) : undefined,
        incomingUpdatedAt,
        existingUpdatedAt,
      });
      ack(ws, id);
      return { acked: true, wrote: false };
    }
    upsertAccountRaw(mergeAccount(incoming, existing));
    logger.info('mutate:account upsert wrote', {
      incoming: summarizeAccountForLog(incoming),
      existing: existing ? summarizeAccountForLog(existing) : undefined,
      incomingUpdatedAt,
      existingUpdatedAt,
    });
    ack(ws, id);
    return { acked: true, wrote: true };
  }

  if (op === 'delete') {
    const name = typeof f.name === 'string' ? f.name : '';
    const incomingUpdatedAt = typeof f.updatedAt === 'number' ? f.updatedAt : 0;
    if (!name) {
      err(ws, id, 'mutate:account delete missing name');
      return { acked: true, wrote: false };
    }
    const existing = loadConfig().accounts.find((a) => a.name === name);
    if (existing && effectiveUpdatedAt(existing) >= incomingUpdatedAt) {
      // Stale; ack but don't write.
      logger.info('mutate:account delete ignored stale', {
        name,
        existing: summarizeAccountForLog(existing),
        incomingUpdatedAt,
        existingUpdatedAt: effectiveUpdatedAt(existing),
      });
      ack(ws, id);
      return { acked: true, wrote: false };
    }
    tombstoneAccount(name, incomingUpdatedAt);
    logger.info('mutate:account delete wrote', {
      name,
      existing: existing ? summarizeAccountForLog(existing) : undefined,
      incomingUpdatedAt,
      existingUpdatedAt: effectiveUpdatedAt(existing),
    });
    ack(ws, id);
    return { acked: true, wrote: true };
  }

  err(ws, id, `mutate:account unknown op: ${String(op)}`);
  return { acked: true, wrote: false };
}
