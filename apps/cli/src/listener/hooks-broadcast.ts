// Listener-side state:hooks broadcaster. Watches ~/.tealstreet/hooks.json
// and pushes the SCRUBBED (no secret field) snapshot to every WS subscribed
// to the 'hooks' topic. Reuses the account-broadcast pattern: fs.watch +
// 250ms debounce + content-hash dedup + notifyOwnWrite for self-write
// echo suppression.
//
// SECURITY: never broadcast Hook.secret. Always go through loadPublicHooks
// (or scrubSecret) before publishing.

import { watch, type FSWatcher } from 'fs';
import type { ServerWebSocket } from 'bun';
import { getHooksFilePath, loadPublicHooks } from './hooks-store';
import { logger } from '../logger';
import type { WsSessionData } from './ws';

const HOOKS_TOPIC = 'hooks';
const DEBOUNCE_MS = 250;

type ServerLike = { publish(topic: string, data: string): number };

function snapshotFrame(): string {
  return JSON.stringify({ t: 'state:hooks', hooks: loadPublicHooks() });
}

function hooksHash(): string {
  return String(Bun.hash(JSON.stringify(loadPublicHooks())));
}

export function sendHooksSnapshot(ws: ServerWebSocket<WsSessionData>): void {
  try {
    ws.send(snapshotFrame());
  } catch (err) {
    logger.warn(`state:hooks send failed: ${(err as Error).message}`);
  }
}

export interface HooksBroadcastHandle {
  notifyOwnWrite(): void;
  stop(): void;
}

export function startHooksBroadcast(server: ServerLike): HooksBroadcastHandle {
  let lastHash = hooksHash();
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let watcher: FSWatcher | null = null;

  const broadcast = () => {
    const hash = hooksHash();
    if (hash === lastHash) return;
    lastHash = hash;
    const frame = snapshotFrame();
    const n = server.publish(HOOKS_TOPIC, frame);
    logger.info(`state:hooks broadcast (${n} bytes published)`);
  };

  const onChange = () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      debounceTimer = null;
      try {
        broadcast();
      } catch (err) {
        logger.warn(`state:hooks broadcast failed: ${(err as Error).message}`);
      }
    }, DEBOUNCE_MS);
  };

  try {
    watcher = watch(getHooksFilePath(), { persistent: false }, onChange);
  } catch (err) {
    // fs.watch can fail (e.g. file doesn't exist yet — hooks.json is only
    // created on first create). The snapshot send on welcome still works.
    logger.warn(`hooks watcher failed to start: ${(err as Error).message}`);
  }

  return {
    notifyOwnWrite() {
      try {
        broadcast();
      } catch (err) {
        logger.warn(`state:hooks broadcast failed: ${(err as Error).message}`);
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

export { HOOKS_TOPIC };
