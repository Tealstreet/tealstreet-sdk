// Listener-side state:accounts broadcaster. Watches ~/.tealstreet/config.json
// and pushes the current accounts snapshot to every WS subscribed to the
// 'accounts' topic.
//
// Why content-hash dedup: fs.watch on macOS fires multiple events per save
// (rename + change for atomic writes), and the listener's own writes via
// handleMutateAccount also trip the watcher. We hash JSON.stringify(accounts)
// after each potential broadcast and skip if it matches the last broadcast —
// that suppresses self-write echo without a polling fallback.
//
// notifyOwnWrite() runs immediately after our own writes so the user-facing
// latency stays at "as fast as fs is settled", not "fs.watch debounce + 250ms".
// The fs.watch event still fires later and finds the hash unchanged → skips.

import type { ServerWebSocket } from 'bun';
import { watch, type FSWatcher } from 'fs';
import { getConfigFilePath, listAllAccounts } from '../config';
import { logger } from '../logger';
import type { WsSessionData } from './ws';

type ServerLike = { publish(topic: string, data: string): number };

const ACCOUNTS_TOPIC = 'accounts';
const DEBOUNCE_MS = 250;

function snapshotFrame(): string {
  return JSON.stringify({ t: 'state:accounts', accounts: listAllAccounts() });
}

function accountsHash(): string {
  // Bun.hash returns a number/bigint; toString() is fine for equality.
  return String(Bun.hash(JSON.stringify(listAllAccounts())));
}

export function sendAccountsSnapshot(ws: ServerWebSocket<WsSessionData>): void {
  try {
    ws.send(snapshotFrame());
  } catch (err) {
    logger.warn(`state:accounts send failed: ${(err as Error).message}`);
  }
}

export interface AccountsBroadcastHandle {
  notifyOwnWrite(): void;
  stop(): void;
}

export function startAccountsBroadcast(server: ServerLike): AccountsBroadcastHandle {
  let lastHash = accountsHash();
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let watcher: FSWatcher | null = null;

  const broadcast = () => {
    const hash = accountsHash();
    if (hash === lastHash) return;
    lastHash = hash;
    const frame = snapshotFrame();
    const n = server.publish(ACCOUNTS_TOPIC, frame);
    logger.info(`state:accounts broadcast (${n} bytes published)`);
  };

  const onChange = () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      debounceTimer = null;
      try {
        broadcast();
      } catch (err) {
        logger.warn(`state:accounts broadcast failed: ${(err as Error).message}`);
      }
    }, DEBOUNCE_MS);
  };

  try {
    watcher = watch(getConfigFilePath(), { persistent: false }, onChange);
  } catch (err) {
    // fs.watch can fail (e.g. file doesn't exist yet) — log and continue;
    // notifyOwnWrite still publishes on local mutate, just no external-edit
    // detection until next restart.
    logger.warn(`accounts watcher failed to start: ${(err as Error).message}`);
  }

  return {
    notifyOwnWrite() {
      try {
        broadcast();
      } catch (err) {
        logger.warn(`state:accounts broadcast failed: ${(err as Error).message}`);
      }
    },
    stop() {
      if (debounceTimer) {
        clearTimeout(debounceTimer);
        debounceTimer = null;
      }
      watcher?.close();
      watcher = null;
    },
  };
}

export { ACCOUNTS_TOPIC };
