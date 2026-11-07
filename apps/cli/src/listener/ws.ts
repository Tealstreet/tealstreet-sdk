import type { ServerWebSocket, WebSocketHandler } from 'bun';
import type { StandalonePlatformAdapter } from '../platform-adapter';
import { logger } from '../logger';
import { isOriginAllowed, getUserEmail } from './auth';
import { isTrusted, addTrustedClient } from './state';
import { promptApproval } from './prompt';
import { handleCmdExec, type ProcessCommandFn } from './handlers/cmd';
import { handleMutateAccount } from './handlers/account';
import { handleMutateCrossTrade } from './handlers/cross-trade';
import { HOOK_OUTPUT_TOPIC } from './handlers/hook';
import { sendAccountsSnapshot, ACCOUNTS_TOPIC } from './account-broadcast';
import { sendTasksSnapshot, TASKS_TOPIC } from './tasks-broadcast';
import { sendCrossTradeSnapshot, CROSS_TRADE_TOPIC } from './cross-trade-broadcast';
import { isAccountSyncEnabled } from './config';
import { BRAND } from '../brand';

export interface WsSessionData {
  origin: string;
  clientId?: string;
  label?: string;
  trusted: boolean;
  helloSeen: boolean;
  /**
   * Latest signal from this web client that its in-tab cross-trade leader is
   * running. Drives the CLI-side "both engines active" warning. Defaults
   * false so old clients (no field in hello) don't trigger warnings.
   */
  webCrossTradeActive: boolean;
}

interface HelloFrame {
  t: 'hello';
  clientId: string;
  label?: string;
  webCrossTradeActive?: boolean;
}

function isHello(msg: unknown): msg is HelloFrame {
  return (
    typeof msg === 'object' &&
    msg !== null &&
    (msg as { t?: unknown }).t === 'hello' &&
    typeof (msg as { clientId?: unknown }).clientId === 'string' &&
    (msg as { clientId: string }).clientId.length > 0
  );
}

function sendWelcome(ws: ServerWebSocket<WsSessionData>, listenerId: string): void {
  const email = getUserEmail();
  ws.send(JSON.stringify({ t: 'welcome', listenerId, user: { email } }));
}

function sendRejected(ws: ServerWebSocket<WsSessionData>): void {
  ws.send(JSON.stringify({ t: 'rejected' }));
}

const CROSS_TRADE_WARN_THROTTLE_MS = 60_000;
let lastCrossTradeWarnAt = 0;

/**
 * Print a one-time-per-minute warning when this CLI process is running the
 * cross-trade daemon AND any connected web client reports it is also
 * running cross-trade locally. Warn-only — no auto-disable, no locking.
 */
function maybeWarnCrossTradeBoth(adapter: StandalonePlatformAdapter, ws: ServerWebSocket<WsSessionData>): void {
  if (!ws.data.webCrossTradeActive) return;
  const { isCrossTradeDaemonRunning } = require('../cross-trade-daemon') as typeof import('../cross-trade-daemon');
  const { readFreshHeartbeat } = require('../cross-trade-heartbeat') as typeof import('../cross-trade-heartbeat');
  // Daemon may be in another process (REPL listener + standalone daemon).
  // Treat either in-process running OR a fresh heartbeat as "CLI engine on".
  const cliRunning = isCrossTradeDaemonRunning() || readFreshHeartbeat() !== null;
  if (!cliRunning) return;
  const now = Date.now();
  if (now - lastCrossTradeWarnAt < CROSS_TRADE_WARN_THROTTLE_MS) return;
  lastCrossTradeWarnAt = now;
  const label = ws.data.label || ws.data.clientId?.slice(0, 8) || '(unknown)';
  adapter.printWarning(
    `cross-trade warning: web client ${label} is also running cross-trade — both engines will place orders. ` +
      `Disable one (web's leader-tab or "${BRAND.binName} cross-trade run") to avoid double-trading.`,
  );
}

export function buildWebSocketHandler({
  adapter,
  listenerId,
  processCommand,
  notifyOwnAccountWrite,
  notifyOwnCrossTradeWrite,
  onClientConnect,
}: {
  adapter: StandalonePlatformAdapter;
  listenerId: string;
  processCommand: ProcessCommandFn;
  notifyOwnAccountWrite: () => void;
  notifyOwnCrossTradeWrite: () => void;
  onClientConnect?: () => void;
}): WebSocketHandler<WsSessionData> {
  const onTrusted = (ws: ServerWebSocket<WsSessionData>) => {
    if (isAccountSyncEnabled()) {
      ws.subscribe(ACCOUNTS_TOPIC);
      sendAccountsSnapshot(ws);
    }
    ws.subscribe(TASKS_TOPIC);
    sendTasksSnapshot(ws);
    ws.subscribe(HOOK_OUTPUT_TOPIC);
    ws.subscribe(CROSS_TRADE_TOPIC);
    sendCrossTradeSnapshot(ws);
  };
  return {
    open(ws) {
      if (!isOriginAllowed(ws.data.origin)) {
        logger.warn(`listener: origin-blocked ${ws.data.origin || '(no Origin header)'}`);
        ws.close(4002, 'origin-blocked');
        return;
      }
      logger.info(`listener: client opened from ${ws.data.origin}`);
      onClientConnect?.();
    },

    async message(ws, raw) {
      let parsed: unknown;
      try {
        parsed = JSON.parse(typeof raw === 'string' ? raw : raw.toString());
      } catch {
        logger.warn('listener: invalid JSON frame');
        return;
      }

      if (!ws.data.helloSeen) {
        if (!isHello(parsed)) {
          logger.warn('listener: first frame was not a valid hello — closing');
          ws.close(4001, 'expected-hello');
          return;
        }
        ws.data.helloSeen = true;
        ws.data.clientId = parsed.clientId;
        ws.data.label = parsed.label ?? '';
        ws.data.webCrossTradeActive = parsed.webCrossTradeActive === true;
        maybeWarnCrossTradeBoth(adapter, ws);

        const known = isTrusted(parsed.clientId);
        if (known) {
          ws.data.trusted = true;
          sendWelcome(ws, listenerId);
          onTrusted(ws);
          logger.info(`listener: welcomed known client ${parsed.clientId.slice(0, 8)}`);
          return;
        }

        const approved = await promptApproval({
          adapter,
          clientId: parsed.clientId,
          label: ws.data.label,
          origin: ws.data.origin,
        });

        if (approved) {
          try {
            addTrustedClient({
              clientId: parsed.clientId,
              label: ws.data.label,
              origin: ws.data.origin,
            });
          } catch (err) {
            logger.error(`listener: failed to persist trusted client: ${(err as Error).message}`);
            sendRejected(ws);
            ws.close(4001, 'persist-failed');
            return;
          }
          ws.data.trusted = true;
          sendWelcome(ws, listenerId);
          onTrusted(ws);
          adapter.printSuccess(`Approved client ${parsed.clientId.slice(0, 8)}…`);
          return;
        }

        sendRejected(ws);
        ws.close(4001, 'rejected');
        return;
      }

      // Post-hello dispatch.
      if (!ws.data.trusted) {
        logger.warn('listener: post-hello frame from untrusted ws — closing');
        ws.close(4001, 'untrusted');
        return;
      }
      const t = (parsed as { t?: unknown }).t;
      if (t === 'cmd:exec') {
        const id = (parsed as { id?: unknown }).id;
        const command = (parsed as { command?: unknown }).command;
        const account = (parsed as { account?: unknown }).account;
        const symbol = (parsed as { symbol?: unknown }).symbol;
        if (typeof id !== 'string' || id.length === 0) {
          logger.warn('listener: cmd:exec missing id');
          return;
        }
        if (typeof command !== 'string') {
          ws.send(JSON.stringify({ t: 'cmd:done', id, success: false, error: 'cmd:exec missing command' }));
          return;
        }
        void handleCmdExec(
          ws,
          id,
          command,
          processCommand,
          typeof account === 'string' && account.length > 0 ? account : undefined,
          typeof symbol === 'string' && symbol.length > 0 ? symbol : undefined,
        );
        return;
      }
      if (t === 'mutate:account') {
        const result = handleMutateAccount(ws, parsed);
        if (result.wrote) notifyOwnAccountWrite();
        return;
      }
      if (t === 'mutate:cross-trade-config') {
        const result = handleMutateCrossTrade(ws, parsed);
        if (result.wrote) notifyOwnCrossTradeWrite();
        return;
      }
      if (t === 'client:state') {
        // Live update to the per-session feature signals carried in hello.
        // Only `webCrossTradeActive` is used today; other fields ignored
        // forward-compat.
        const next = (parsed as { webCrossTradeActive?: unknown }).webCrossTradeActive;
        if (typeof next === 'boolean' && next !== ws.data.webCrossTradeActive) {
          ws.data.webCrossTradeActive = next;
          maybeWarnCrossTradeBoth(adapter, ws);
        }
        return;
      }
      logger.info(`listener: unhandled post-hello frame t=${String(t)}`);
    },

    close(ws, code, reason) {
      const id = ws.data.clientId ? ws.data.clientId.slice(0, 8) : '(no hello)';
      logger.info(`listener: client closed ${id} code=${code} reason=${reason || ''}`);
    },
  };
}
