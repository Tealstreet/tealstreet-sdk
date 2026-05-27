// Listener-side state:tasks broadcaster. Watches ~/.tealstreet/tasks.json
// and pushes the current tasks snapshot to every WS subscribed to the
// 'tasks' topic. Reuses the account-broadcast pattern (fs.watch + 250ms
// debounce + content-hash dedup), minus the notifyOwnWrite slot because
// task lifecycle is driven only by cmd:exec running `kill <id>` /
// `tasks clear` etc. — no inbound mutate:tasks frame to short-circuit
// the fs.watch latency.

import { watch, type FSWatcher } from 'fs';
import type { ServerWebSocket } from 'bun';
import { loadTasks } from '../tasks-store';
import { logger } from '../logger';
import type { WsSessionData } from './ws';

const TASKS_TOPIC = 'tasks';
const DEBOUNCE_MS = 250;

// File path mirrored here to avoid importing the runtime path from
// tasks-store (which doesn't export it directly). Kept in sync with
// tasks-store.ts:TASKS_FILE.
import { TASKS_FILE } from '../tasks-store';

type ServerLike = { publish(topic: string, data: string): number };

function snapshotFrame(): string {
  return JSON.stringify({ t: 'state:tasks', tasks: loadTasks() });
}

function tasksHash(): string {
  return String(Bun.hash(JSON.stringify(loadTasks())));
}

export function sendTasksSnapshot(ws: ServerWebSocket<WsSessionData>): void {
  try {
    ws.send(snapshotFrame());
  } catch (err) {
    logger.warn(`state:tasks send failed: ${(err as Error).message}`);
  }
}

export interface TasksBroadcastHandle {
  stop(): void;
}

export function startTasksBroadcast(server: ServerLike): TasksBroadcastHandle {
  let lastHash = tasksHash();
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let watcher: FSWatcher | null = null;

  const broadcast = () => {
    const hash = tasksHash();
    if (hash === lastHash) return;
    lastHash = hash;
    const frame = snapshotFrame();
    const n = server.publish(TASKS_TOPIC, frame);
    logger.info(`state:tasks broadcast (${n} bytes published)`);
  };

  const onChange = () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      debounceTimer = null;
      try {
        broadcast();
      } catch (err) {
        logger.warn(`state:tasks broadcast failed: ${(err as Error).message}`);
      }
    }, DEBOUNCE_MS);
  };

  try {
    watcher = watch(TASKS_FILE, { persistent: false }, onChange);
  } catch (err) {
    // fs.watch can fail (e.g. file doesn't exist yet). Logged + continue —
    // subsequent file creation won't be caught until next restart, but the
    // snapshot send on welcome still works.
    logger.warn(`tasks watcher failed to start: ${(err as Error).message}`);
  }

  return {
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

export { TASKS_TOPIC };
