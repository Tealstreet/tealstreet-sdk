// File-based heartbeat written by the cross-trade daemon process and read
// by any CLI listener process (REPL or otherwise). This is what makes the
// daemon's running state visible to a web tab even when the daemon and the
// REPL are different processes.
//
// Shape lives at ~/.tealstreet/cross-trade-running.json:
//   { pid: number, startedAt: number, configCount: number,
//     lastHeartbeatAt: number, version: 1 }
//
// The daemon updates lastHeartbeatAt every HEARTBEAT_INTERVAL_MS. Readers
// treat the heartbeat as stale if lastHeartbeatAt is older than STALE_MS,
// at which point the daemon is presumed dead (likely a hard kill / OOM)
// and the broadcast reports running=false even if the file is still on
// disk. The daemon deletes the file on graceful shutdown.

import { existsSync, mkdirSync, readFileSync, unlinkSync, writeFileSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import { BRAND } from './brand';

const CONFIG_DIR = process.env.TEALSTREET_CONFIG_DIR || join(homedir(), BRAND.configDirName);
const HEARTBEAT_FILE = join(CONFIG_DIR, 'cross-trade-running.json');

export const HEARTBEAT_INTERVAL_MS = 5_000;
// 3× the interval gives the watcher comfort room around fs latency.
export const STALE_MS = 15_000;

export interface CrossTradeHeartbeat {
  version: 1;
  pid: number;
  startedAt: number;
  configCount: number;
  lastHeartbeatAt: number;
}

export function getHeartbeatFilePath(): string {
  return HEARTBEAT_FILE;
}

function ensureDir(): void {
  if (!existsSync(CONFIG_DIR)) {
    mkdirSync(CONFIG_DIR, { recursive: true });
  }
}

export function writeHeartbeat(input: { startedAt: number; configCount: number }): void {
  ensureDir();
  const data: CrossTradeHeartbeat = {
    version: 1,
    pid: process.pid,
    startedAt: input.startedAt,
    configCount: input.configCount,
    lastHeartbeatAt: Date.now(),
  };
  writeFileSync(HEARTBEAT_FILE, JSON.stringify(data, null, 2));
}

export function clearHeartbeat(): void {
  try {
    if (existsSync(HEARTBEAT_FILE)) {
      unlinkSync(HEARTBEAT_FILE);
    }
  } catch {
    // best effort — listener falls back to staleness check
  }
}

export function readHeartbeat(): CrossTradeHeartbeat | null {
  if (!existsSync(HEARTBEAT_FILE)) return null;
  try {
    const parsed = JSON.parse(readFileSync(HEARTBEAT_FILE, 'utf-8')) as CrossTradeHeartbeat;
    if (parsed.version !== 1) return null;
    if (typeof parsed.lastHeartbeatAt !== 'number') return null;
    return parsed;
  } catch {
    return null;
  }
}

/**
 * Returns the current heartbeat IFF it's fresh. Stale heartbeats (daemon
 * died without cleanup) are treated as not-running.
 */
export function readFreshHeartbeat(now: number = Date.now()): CrossTradeHeartbeat | null {
  const hb = readHeartbeat();
  if (!hb) return null;
  if (now - hb.lastHeartbeatAt > STALE_MS) return null;
  return hb;
}
