// Listener-side state:cross-trade broadcaster.
//
// Two inputs feed the snapshot:
//   1. ~/.tealstreet/cross-trade-running.json — written by the daemon
//      process; reports `running: true`, the daemon's pid, configCount, and
//      the latest heartbeat timestamp. Stale heartbeats (>STALE_MS) are
//      treated as not-running so a hard-killed daemon doesn't leave the
//      web UI showing a phantom running engine.
//   2. ~/.tealstreet/cross-trade.json — the config registry; broadcast on
//      change so web sees enable/disable/add/remove edits from the CLI.
//
// fs.watch fires on both files; debounced + content-hash deduped to absorb
// macOS's double-fire and the daemon's own heartbeat thrash.

import type { ServerWebSocket } from 'bun';
import { watch, type FSWatcher } from 'fs';

import { getCrossTradeFilePath, listAllCrossTradeConfigs } from '../cross-trade-store';
import { getHeartbeatFilePath, readFreshHeartbeat } from '../cross-trade-heartbeat';
import { logger } from '../logger';
import type { WsSessionData } from './ws';

type ServerLike = { publish(topic: string, data: string): number };

export const CROSS_TRADE_TOPIC = 'cross-trade';
const DEBOUNCE_MS = 250;
// Re-check the heartbeat at the staleness boundary so a daemon that died
// without writing a final heartbeat decays out of the snapshot promptly
// even if no fs event fires.
const STALENESS_RECHECK_MS = 5_000;

interface CrossTradeSnapshot {
  t: 'state:cross-trade';
  running: boolean;
  pid?: number;
  startedAt?: number;
  configCount: number;
  configs: ReturnType<typeof listAllCrossTradeConfigs>;
}

function buildSnapshot(): CrossTradeSnapshot {
  const hb = readFreshHeartbeat();
  const configs = listAllCrossTradeConfigs();
  if (hb) {
    return {
      t: 'state:cross-trade',
      running: true,
      pid: hb.pid,
      startedAt: hb.startedAt,
      configCount: hb.configCount,
      configs,
    };
  }
  return {
    t: 'state:cross-trade',
    running: false,
    configCount: 0,
    configs,
  };
}

function snapshotHash(snap: CrossTradeSnapshot): string {
  return String(Bun.hash(JSON.stringify(snap)));
}

export function sendCrossTradeSnapshot(ws: ServerWebSocket<WsSessionData>): void {
  try {
    const snap = buildSnapshot();
    ws.send(JSON.stringify(snap));
    logger.info('state:cross-trade send', { running: snap.running, configCount: snap.configCount });
  } catch (err) {
    logger.warn(`state:cross-trade send failed: ${(err as Error).message}`);
  }
}

export interface CrossTradeBroadcastHandle {
  notifyOwnWrite(): void;
  stop(): void;
}

export function startCrossTradeBroadcast(server: ServerLike): CrossTradeBroadcastHandle {
  let lastHash = snapshotHash(buildSnapshot());
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let watchers: FSWatcher[] = [];
  let stalenessTimer: ReturnType<typeof setInterval> | null = null;

  const broadcast = () => {
    const snap = buildSnapshot();
    const hash = snapshotHash(snap);
    if (hash === lastHash) return;
    lastHash = hash;
    const n = server.publish(CROSS_TRADE_TOPIC, JSON.stringify(snap));
    logger.info(`state:cross-trade broadcast (${n} bytes)`, {
      running: snap.running,
      configCount: snap.configCount,
    });
  };

  const onChange = () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      debounceTimer = null;
      try {
        broadcast();
      } catch (err) {
        logger.warn(`state:cross-trade broadcast failed: ${(err as Error).message}`);
      }
    }, DEBOUNCE_MS);
  };

  for (const path of [getCrossTradeFilePath(), getHeartbeatFilePath()]) {
    try {
      watchers.push(watch(path, { persistent: false }, onChange));
    } catch (err) {
      // The file may not exist yet (daemon not started). The staleness
      // recheck timer below will pick up the heartbeat once it appears.
      logger.warn(`cross-trade watcher (${path}) failed: ${(err as Error).message}`);
    }
  }

  stalenessTimer = setInterval(() => {
    try {
      broadcast();
    } catch (err) {
      logger.warn(`state:cross-trade staleness recheck failed: ${(err as Error).message}`);
    }
  }, STALENESS_RECHECK_MS);

  return {
    notifyOwnWrite() {
      try {
        broadcast();
      } catch (err) {
        logger.warn(`state:cross-trade broadcast failed: ${(err as Error).message}`);
      }
    },
    stop() {
      if (debounceTimer) {
        clearTimeout(debounceTimer);
        debounceTimer = null;
      }
      if (stalenessTimer) {
        clearInterval(stalenessTimer);
        stalenessTimer = null;
      }
      for (const w of watchers) {
        try {
          w.close();
        } catch {
          /* ignore */
        }
      }
      watchers = [];
    },
  };
}
