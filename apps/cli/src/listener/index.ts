import type { StandalonePlatformAdapter } from '../platform-adapter';
import { existsSync, mkdirSync, writeFileSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import { buildServeOptions } from './http';
import { getListenerId } from './state';
import { isListenerEnabled, isAccountSyncEnabled } from './config';
import type { ProcessCommandFn } from './handlers/cmd';
import { startAccountsBroadcast, type AccountsBroadcastHandle } from './account-broadcast';
import { startTasksBroadcast, type TasksBroadcastHandle } from './tasks-broadcast';
import { startCrossTradeBroadcast, type CrossTradeBroadcastHandle } from './cross-trade-broadcast';
import { gcTombstonedAccounts, getConfigFilePath } from '../config';
import { gcTombstonedCrossTradeConfigs, getCrossTradeFilePath } from '../cross-trade-store';
import { TASKS_FILE } from '../tasks-store';
import { getHooksFilePath } from './hooks-store';
import { logger } from '../logger';
import { BRAND } from '../brand';

// `fs.watch` throws permanently if the target file doesn't exist on bind.
// Seed each broadcaster's target with a valid empty shape on startup so
// the watcher attaches and subsequent edits (CLI or external) fire events.
function ensureStateFilesExist(): void {
  const dir = join(homedir(), BRAND.configDirName);
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true, mode: 0o700 });
  const seeds: Array<[string, string]> = [
    [getConfigFilePath(), JSON.stringify({ accounts: [] }, null, 2)],
    [TASKS_FILE, JSON.stringify({ version: 1, tasks: [] }, null, 2)],
    [getHooksFilePath(), JSON.stringify({ hooks: [] }, null, 2)],
    [getCrossTradeFilePath(), JSON.stringify({ configs: [] }, null, 2)],
  ];
  for (const [path, seed] of seeds) {
    if (!existsSync(path)) {
      try {
        // config.json and hooks.json later hold credentials.
        writeFileSync(path, seed, { mode: 0o600 });
      } catch (err) {
        logger.warn(`listener: failed to seed ${path}: ${(err as Error).message}`);
      }
    }
  }
}

const DEFAULT_PORT = 53219;

// Web's reconnect ceiling is 60s (apps/web/src/components/cliListener/CliListenerWorker.tsx).
// 75s gives the worst-case web attempt one full window plus a small buffer
// before we prompt the user to refresh their tab. If the user just started
// the CLI and their web tab has been open for hours (sitting at the 60s
// cap), this catches the "where's the connect prompt?" case.
const CLIENT_HINT_DELAY_MS = 75_000;

let server: ReturnType<typeof Bun.serve> | null = null;
let accountsBroadcast: AccountsBroadcastHandle | null = null;
let tasksBroadcast: TasksBroadcastHandle | null = null;
let crossTradeBroadcast: CrossTradeBroadcastHandle | null = null;
let clientHintTimer: ReturnType<typeof setTimeout> | null = null;

export async function startListener({
  adapter,
  processCommand,
  port = DEFAULT_PORT,
}: {
  adapter: StandalonePlatformAdapter;
  processCommand: ProcessCommandFn;
  port?: number;
}): Promise<void> {
  if (server) return;
  if (!isListenerEnabled()) {
    adapter.printInfo('listener disabled (config.listener.enabled=false)');
    return;
  }
  ensureStateFilesExist();
  if (isAccountSyncEnabled()) {
    const removed = gcTombstonedAccounts();
    if (removed > 0) logger.info(`startup: GC'd ${removed} tombstoned account(s)`);
  }
  try {
    const removed = gcTombstonedCrossTradeConfigs();
    if (removed > 0) logger.info(`startup: GC'd ${removed} tombstoned cross-trade config(s)`);
  } catch (err) {
    logger.warn(`cross-trade tombstone GC failed: ${(err as Error).message}`);
  }
  try {
    const listenerId = getListenerId();
    // Bootstrap loop: build serve options needs a notifier; the broadcast
    // needs the server it's notifying. Wire via a small mutable bridge.
    let broadcast: AccountsBroadcastHandle | null = null;
    let crossTrade: CrossTradeBroadcastHandle | null = null;
    const notifyOwnAccountWrite = () => broadcast?.notifyOwnWrite();
    const notifyOwnCrossTradeWrite = () => crossTrade?.notifyOwnWrite();
    const onClientConnect = () => {
      if (clientHintTimer) {
        clearTimeout(clientHintTimer);
        clientHintTimer = null;
      }
    };
    server = Bun.serve(
      buildServeOptions({
        port,
        adapter,
        listenerId,
        processCommand,
        notifyOwnAccountWrite,
        notifyOwnCrossTradeWrite,
        onClientConnect,
      }),
    );
    broadcast = startAccountsBroadcast(server);
    accountsBroadcast = broadcast;
    tasksBroadcast = startTasksBroadcast(server);
    crossTrade = startCrossTradeBroadcast(server);
    crossTradeBroadcast = crossTrade;
    adapter.printInfo(`listener on http://127.0.0.1:${port}`);
    clientHintTimer = setTimeout(() => {
      clientHintTimer = null;
      adapter.printInfo(
        "listener: still waiting for a web client. If you have ${BRAND.displayName} open in a browser tab and the connect prompt hasn't appeared, try refreshing that tab.",
      );
    }, CLIENT_HINT_DELAY_MS);
  } catch (err) {
    server = null;
    accountsBroadcast = null;
    tasksBroadcast = null;
    crossTradeBroadcast = null;
    const msg = (err as Error)?.message ?? String(err);
    adapter.printError(`listener failed to bind on 127.0.0.1:${port}: ${msg}`);
  }
}

export function stopListener(): void {
  if (clientHintTimer) {
    clearTimeout(clientHintTimer);
    clientHintTimer = null;
  }
  accountsBroadcast?.stop();
  accountsBroadcast = null;
  tasksBroadcast?.stop();
  tasksBroadcast = null;
  crossTradeBroadcast?.stop();
  crossTradeBroadcast = null;
  if (!server) return;
  server.stop(true);
  server = null;
}
