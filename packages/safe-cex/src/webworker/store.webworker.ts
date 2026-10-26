import { ExchangeName } from '../types';
import { setTickIntervalMultipliers } from '../utils/tickIntervalOverride';
import { virtualClock } from '../utils/virtualClock';
import { setWsDiagnosticsEnabled } from '../utils/wsDiagnostics';
import { ExchangeWorker } from './ExchangeWorker';
import { setMainAcceptsFrames } from './frameForwarding';
import {
  configurePerfFixture,
  isReplayingPerfFixture,
  recordOutboundFrame,
  startReplay,
  takePerfFixture,
} from './perfFixture';
import { StoreWorkerPayload, WorkerConfig } from './StoreWorkerPayload';
import { TelegramWorker } from './TelgramWorker';
import { serializeWorkerErrorForSentry } from './workerErrorSerialization';

export const anonymousExchangeWorkers: { [key: string]: ExchangeWorker } = {};
export const exchangeWorkers: { [key: string]: ExchangeWorker } = {};
const ports: { [key: string]: (MessagePort | Worker)[] } = {};
const portsBySubId: { [key: string]: MessagePort | Worker } = {};

// Per-port state for throttling (SharedWorker per-client throttling)
interface PortState {
  visible: boolean;
  throttleEnabled: boolean;
  lastDeliveryTime: number;
  pendingUpdate: unknown;
  throttleTimer: ReturnType<typeof setTimeout> | null;
}
const portStates: Map<MessagePort | Worker, PortState> = new Map();

// Throttle interval for hidden clients only
// Visible clients: no throttle here - ExchangeWorker already throttles at 250ms
const THROTTLE_HIDDEN_MS = 2500;

type WorkerConfigHandler = (cfg: Record<string, unknown>) => void;
const workerConfigHandlers: WorkerConfigHandler[] = [];

// Host-specific '__workerConfig' keys (e.g. Tealstreet's WS relay flag) are
// applied by the host's own handler; register it before any message arrives.
export const registerWorkerConfigHandler = (handler: WorkerConfigHandler): void => {
  workerConfigHandlers.push(handler);
};

function cleanupPortEverywhere(port: MessagePort | Worker): void {
  cleanupPortState(port);

  for (const key of Object.keys(ports)) {
    ports[key] = ports[key].filter((candidate) => candidate !== port);
    if (ports[key].length === 0) {
      delete ports[key];
    }
  }

  for (const subId of Object.keys(portsBySubId)) {
    if (portsBySubId[subId] === port) {
      delete portsBySubId[subId];
    }
  }
}

/**
 * Deliver a message to a single port with throttling applied.
 * Only throttles hidden clients - visible clients get immediate delivery.
 */
function deliverToPortWithThrottle(port: MessagePort | Worker, msg: unknown): void {
  let state = portStates.get(port);
  if (!state) {
    state = { visible: true, throttleEnabled: false, lastDeliveryTime: 0, pendingUpdate: null, throttleTimer: null };
    portStates.set(port, state);
  }

  // Frames carry deltas, so a hidden tab must still receive every one.
  const isFrame = (msg as { op?: string } | null)?.op === 'frame';
  const shouldThrottle = state.throttleEnabled && !state.visible && !isFrame;

  // Visible clients: deliver immediately, no throttling
  // ExchangeWorker already throttles at 250ms
  if (!shouldThrottle) {
    try {
      port.postMessage(msg);
    } catch (_error) {
      cleanupPortEverywhere(port);
      return;
    }
    // Clear any pending hidden update if client became visible
    if (state.throttleTimer) {
      clearTimeout(state.throttleTimer);
      state.throttleTimer = null;
    }
    state.pendingUpdate = null;
    return;
  }

  // Hidden clients: apply throttling
  const now = Date.now();
  const timeSinceLastDelivery = now - state.lastDeliveryTime;

  if (timeSinceLastDelivery >= THROTTLE_HIDDEN_MS) {
    // Enough time has passed, deliver immediately
    state.lastDeliveryTime = now;
    try {
      port.postMessage(msg);
    } catch (_error) {
      cleanupPortEverywhere(port);
      return;
    }

    // Clear any pending update
    if (state.throttleTimer) {
      clearTimeout(state.throttleTimer);
      state.throttleTimer = null;
    }
    state.pendingUpdate = null;
  } else {
    // Too soon, queue for later delivery
    state.pendingUpdate = msg;

    if (!state.throttleTimer) {
      const remainingTime = THROTTLE_HIDDEN_MS - timeSinceLastDelivery;
      state.throttleTimer = setTimeout(() => {
        const currentState = portStates.get(port);
        if (currentState?.pendingUpdate) {
          try {
            port.postMessage(currentState.pendingUpdate);
          } catch (_error) {
            cleanupPortEverywhere(port);
            return;
          }
          currentState.lastDeliveryTime = Date.now();
          currentState.pendingUpdate = null;
        }
        if (currentState) {
          currentState.throttleTimer = null;
        }
      }, remainingTime);
    }
  }
}

/**
 * Broadcast a message to all ports for a given portKey with per-port throttling
 */
function broadcastWithThrottle(portKey: string, msg: unknown, subId?: string): void {
  let validPorts = ports[portKey];
  if (!validPorts) return;

  if (subId) {
    // If subId is present, use portsBySubId to find relevant port
    validPorts = portsBySubId[subId] ? [portsBySubId[subId]] : validPorts;
  }

  for (const port of validPorts) {
    deliverToPortWithThrottle(port, msg);
  }
}

/**
 * Clean up port state (call when port disconnects)
 */
function cleanupPortState(port: MessagePort | Worker): void {
  const state = portStates.get(port);
  if (state?.throttleTimer) {
    clearTimeout(state.throttleTimer);
  }
  portStates.delete(port);
}

const start = (ctx: Worker, shared = false) => {
  let accountName: string;
  let exchangeName: ExchangeName;
  let _initPayload: StoreWorkerPayload;
  let portKey: string;
  let hasLoggedInstanceMismatch = false;
  const _originalPostMessage = !shared
    ? ctx.postMessage.bind(ctx)
    : (msg: any) => {
        // For shared workers, postMessage is handled differently
        if (portKey) {
          broadcastWithThrottle(portKey, msg);
        }
      };

  const getMyExchangeWorker = () =>
    accountName ? exchangeWorkers[accountName] : anonymousExchangeWorkers[exchangeName];

  const deleteMyExchangeWorker = () => {
    if (accountName) {
      delete exchangeWorkers[accountName];
    } else {
      delete anonymousExchangeWorkers[exchangeName];
    }
  };

  const initExchangeWorker = async (payload: StoreWorkerPayload) => {
    _initPayload = payload;
    accountName = payload.data.accountName;
    exchangeName = payload.data.exchangeName;

    // A replay run is driven entirely by the fixture. Opening the venue as well
    // would feed live prices back into the page and reintroduce exactly the
    // variance the fixture exists to remove.
    if (isReplayingPerfFixture()) {
      ctx.postMessage({ op: 'start-status', data: true });
      startReplay(_originalPostMessage);
      return;
    }
    // Use prefixes to avoid collisions between account names and exchange names
    // e.g., account named "binance" vs anonymous binance connection
    portKey = accountName ? `account:${accountName}` : exchangeName ? `exchange:${exchangeName}` : '';

    // Single choke point for everything this worker sends the main thread.
    // `ExchangeWorker` posts through `ctxWrapper`, not `_originalPostMessage`,
    // so recording has to sit here or it captures nothing on dedicated workers.
    // `recordOutboundFrame` is a boolean check when the harness is off.
    let ctxWrapper = new Proxy(ctx, {
      get(target, prop, receiver) {
        if (prop === 'postMessage') {
          return function (msg: any, transfer?: Transferable[]) {
            recordOutboundFrame(msg);
            return transfer ? (target as any).postMessage(msg, transfer) : (target as any).postMessage(msg);
          };
        }
        return Reflect.get(target, prop, receiver);
      },
    });

    if (shared) {
      if (!ports[portKey]) ports[portKey] = [];
      if (!ports[portKey].includes(ctx)) {
        ports[portKey].push(ctx);
      }
      const existingWorker = getMyExchangeWorker();
      if (existingWorker) {
        // Check if the exchange is still alive (not disposed)
        if (existingWorker.isRecovering) {
          ctx.postMessage({ op: 'start-status', data: false });
          ctx.postMessage({
            op: 'warning',
            data: ['Rust engine recovery is in progress; account commands are temporarily unavailable.'],
          });
          return;
        } else if (existingWorker.exchange?.isDisposed) {
          existingWorker.dispose();
          deleteMyExchangeWorker();
        } else {
          ctx.postMessage({ op: 'start-status', data: true });
          existingWorker.sendSnapshot();
          return;
        }
      }
      // Wrap postMessage to broadcast to all ports with per-port throttling
      ctxWrapper = new Proxy(ctx, {
        get(target, prop) {
          if (prop === 'postMessage') {
            return function (msg: any) {
              recordOutboundFrame(msg);
              const subId = msg?.data?.subId;
              broadcastWithThrottle(portKey, msg, subId);
            };
          }
          return Reflect.get(target, prop);
        },
      });
    }

    const options = payload.data.options;
    const settings = payload.data.settings;
    const metadata = payload.data.metadata;
    const sessionKey = payload.data.sessionKey;

    const exchangeWorker = new ExchangeWorker(
      ctxWrapper,
      accountName,
      exchangeName,
      options,
      settings,
      metadata,
      sessionKey,
      payload.instanceId,
    );

    if (accountName) {
      exchangeWorkers[accountName] = exchangeWorker;
    } else {
      anonymousExchangeWorkers[exchangeName] = exchangeWorker;
    }

    // Seed the worker store with the main-thread's serialized state BEFORE
    // starting the exchange. The exchange's fetchMarkets() will then populate
    // markets/tickers on top. Seeding AFTER start() would race and could wipe
    // freshly-loaded markets when the main-thread store is empty (cold load,
    // no IndexedDB cache, default-demo, etc.).
    exchangeWorker.store.update(payload.data.store);

    const started = await exchangeWorker.start();
    if (!started) {
      // Startup was cancelled (e.g. dispose mid-flight) or failed. Drop the
      // worker from the map so a subsequent init creates a fresh one instead
      // of reusing a half-initialized instance.
      if (getMyExchangeWorker() === exchangeWorker) {
        deleteMyExchangeWorker();
      }
      exchangeWorker.dispose();
      return;
    }
  };

  ctx.addEventListener('message', async (event: any) => {
    const payload = event.data as StoreWorkerPayload;

    // Heartbeat — answer before any init / instanceId checks so a worker that's
    // alive but has not finished init (or has a stale instanceId) still proves
    // it's running. Main thread uses this to detect a dead/hung worker.
    if (payload?.op === '__worker_ping__') {
      try {
        ctx.postMessage({
          op: '__worker_pong__',
          instanceId: payload.instanceId,
          data: { ts: Date.now() },
        });
      } catch (_e) {
        // ignore — if postMessage throws, the port is dead anyway
      }
      return;
    }

    // Global worker config: the host app derives these knobs from per-user feature flags
    // and pushes them once, before any 'init', so safe-cex resolves the right hosts from
    // the start. Applied via typed setters — safe-cex never sees flag names.
    if (payload?.op === '__workerConfig') {
      const cfg = (payload.data ?? {}) as WorkerConfig;
      if ('tickIntervalMultipliers' in cfg) setTickIntervalMultipliers(cfg.tickIntervalMultipliers);
      if ('wsDiagnosticsEnabled' in cfg) setWsDiagnosticsEnabled(cfg.wsDiagnosticsEnabled === true);
      if ('acceptsFrames' in cfg) setMainAcceptsFrames(cfg.acceptsFrames === true);
      for (const handler of workerConfigHandlers) {
        try {
          handler(cfg as Record<string, unknown>);
        } catch (e) {
          console.error('[store.webworker] __workerConfig host handler threw', e);
        }
      }
      if ('perfFixture' in cfg) {
        configurePerfFixture(cfg.perfFixture);
        // Replay drives the main thread by itself — see `perfFixture.ts`. It is
        // started here rather than on 'init' because a replay run never creates
        // an ExchangeWorker, so 'init' short-circuits before reaching it.
        if (isReplayingPerfFixture()) startReplay(_originalPostMessage);
      }
      return;
    }

    // Dev-only: hand the recorded fixture back so the page can save it.
    if (payload?.op === '__perfFixtureDump') {
      try {
        _originalPostMessage({ op: '__perfFixtureDump', data: takePerfFixture() });
      } catch (_e) {
        // ignore — dead port
      }
      return;
    }

    // The browser main thread owns network time synchronization and fans the
    // result out to every local exchange worker. This keeps the request rate
    // per tab, rather than per open account.
    if (payload?.op === 'sync-time') {
      const timeMs = payload.data?.timeMs;
      if (typeof timeMs === 'number' && Number.isFinite(timeMs)) {
        virtualClock.syncFromServerTime(timeMs);
      }
      return;
    }

    let exchangeWorker = getMyExchangeWorker();

    if (
      !shared &&
      exchangeWorker?.instanceId &&
      payload.instanceId &&
      payload.instanceId !== exchangeWorker?.instanceId
    ) {
      if (!hasLoggedInstanceMismatch) {
        hasLoggedInstanceMismatch = true;
        console.warn(
          '[StoreWebWorker] Instance ID mismatch detected; dropping stale messages for this worker lifecycle',
          {
            expectedInstanceId: exchangeWorker.instanceId,
            receivedInstanceId: payload.instanceId,
            payloadOp: payload.op,
            account: accountName || 'unknown',
            exchange: exchangeName || 'unknown',
          },
        );
      }
      return;
    }

    switch (payload.op) {
      case 'init':
        await initExchangeWorker(payload);

        // re-get exchangeWorker after init
        exchangeWorker = getMyExchangeWorker();

        if (
          exchangeWorker?.exchange &&
          !exchangeWorker.exchange.isDisposed &&
          payload.data.telegramBotToken &&
          payload.data.telegramBotToken !== ''
        ) {
          const telegramWorker = new TelegramWorker(() => exchangeWorker!.exchange!, payload.data.telegramBotToken);
          telegramWorker.start();
        }
        break;
      case 'dispose':
        if (shared) {
          cleanupPortEverywhere(ctx);

          const hasRemainingPorts = Boolean(portKey && ports[portKey]?.length > 0);
          if (!hasRemainingPorts && exchangeWorker) {
            deleteMyExchangeWorker();
            exchangeWorker.handlePayload(payload);
          }
        } else {
          if (exchangeWorker) {
            // Drop from the map first so a fresh init creates a new worker
            // rather than reusing this disposing one.
            deleteMyExchangeWorker();
            exchangeWorker.handlePayload(payload);
          }
        }
        break;
      case 'reinit':
        // Force dispose: immediately dispose existing exchange without timeout
        // The web app will send a subsequent 'init' message to create a fresh exchange
        // This avoids the 10s dispose timeout that would cause race conditions
        console.log(`[Worker] 🔄 reinit (force dispose) received for ${accountName || exchangeName}`);
        if (exchangeWorker) {
          console.log(`[Worker] 🗑️ Force disposing existing exchange`);
          deleteMyExchangeWorker();
          exchangeWorker.handlePayload({ op: 'dispose' });
        }
        // Clear ports and port states for this account in shared worker mode
        if (shared && portKey) {
          // Clean up throttle states for all ports being cleared
          for (const port of ports[portKey] || []) {
            cleanupPortState(port);
          }
          ports[portKey] = [];
        }
        console.log(`[Worker] ✓ Exchange disposed, ready for fresh init`);
        break;
      case 'sendSnapshot':
        // CLI serve mode: send full store snapshot to all connected sessions
        // Mirrors SharedWorker behavior (line 194-195) where existingWorker.sendSnapshot()
        // is called directly when a new port connects to an existing exchange
        if (exchangeWorker) {
          exchangeWorker.sendSnapshot();
        }
        break;
      default:
        // case 'store':
        // case 'dispose':
        // case 'subscribeOHLCV':
        // case 'subscribeOrderBook':
        // case 'unsubscribeOHLCV':
        // case 'unsubscribeOrderBook':
        if (exchangeWorker) {
          if (shared && payload?.data?.subId) {
            // For shared workers, store the port by subId for targeted messaging
            portsBySubId[payload.data.subId] = ctx;
          }
          exchangeWorker.handlePayload(payload);
        }
        break;
    }
  });
};

// if in webworker start immediately
if (typeof self !== 'undefined' && !self.document) {
  const shared = 'SharedWorkerGlobalScope' in self;

  // only works if not in shared worker

  // Add global error handler for uncaught exceptions in the worker
  self.addEventListener('error', (event: ErrorEvent) => {
    const error = event.error || event;
    const message = error?.message || event?.message || 'Unknown error';
    const serializedError = serializeWorkerErrorForSentry(error, message);

    // Always prevent default to avoid duplicate reporting
    // event.preventDefault();

    // For all errors, send to main thread with enhanced context
    try {
      // Try to get the account name or exchange name for context
      const accountNames = Object.keys(exchangeWorkers);
      const exchangeNames = Object.keys(anonymousExchangeWorkers);
      const context =
        accountNames.length > 0
          ? `account: ${accountNames.join(',')}`
          : exchangeNames.length > 0
            ? `exchange: ${exchangeNames.join(',')}`
            : 'unknown context';

      // Extract additional context from enhanced errors
      const extra: any = {
        originalMessage: message,
        errorType: serializedError.name,
        context: context,
      };

      // If this is an enhanced WebSocket error, include the extra context
      if (error && typeof error === 'object') {
        if (error.exchange) extra.exchange = error.exchange;
        if (error.webSocketState) extra.webSocketState = error.webSocketState;
        if (error.webSocketReadyState !== undefined) extra.webSocketReadyState = error.webSocketReadyState;
        if (error.messagePreview) extra.messagePreview = error.messagePreview;
        if (error.isReconnecting !== undefined) extra.isReconnecting = error.isReconnecting;
        if (error.originalError) extra.originalErrorMessage = error.originalError?.message;
      }

      const postMessage = (msg: any) => {
        if (!shared) {
          (self as any).postMessage(msg);
        } else {
          const somePort = Object.values(ports).flat()[0];
          if (somePort) {
            somePort.postMessage(msg);
          } else {
            console.error('[Worker] No ports available to send error message', msg);
          }
        }
      };

      // Send error to main thread with all context
      postMessage({
        op: 'error',
        data: [
          message,
          {
            sendToSentry: true,
            sentryData: {
              error: serializedError,
              tags: {
                source: 'webworker-uncaught',
                module: 'StoreWebWorker',
                errorName: serializedError.name,
                ...(error?.exchange ? { exchange: error.exchange } : {}),
                ...(error?.webSocketState ? { wsState: error.webSocketState } : {}),
              },
              extra: extra,
            },
          },
        ],
      });
    } catch (e) {
      console.error('[Worker] Failed to send error to main thread:', e);
    }
  });

  // Unhandled promise rejections — same delivery shape as the `error` listener
  // above so they flow through the existing `op: 'error'` → emitter →
  // handleErrWarning toast/Sentry pipeline.
  self.addEventListener('unhandledrejection', (event: PromiseRejectionEvent) => {
    const reason: any = event?.reason;
    const message =
      typeof reason === 'string' ? reason : reason?.message || (reason ? String(reason) : 'Unhandled rejection');
    const serializedError = serializeWorkerErrorForSentry(reason, message);

    try {
      const accountNames = Object.keys(exchangeWorkers);
      const exchangeNames = Object.keys(anonymousExchangeWorkers);
      const context =
        accountNames.length > 0
          ? `account: ${accountNames.join(',')}`
          : exchangeNames.length > 0
            ? `exchange: ${exchangeNames.join(',')}`
            : 'unknown context';

      const extra: any = {
        originalMessage: message,
        errorType: reason?.name || 'UnhandledRejection',
        context,
      };
      if (reason && typeof reason === 'object') {
        if (reason.exchange) extra.exchange = reason.exchange;
        if (reason.webSocketState) extra.webSocketState = reason.webSocketState;
        if (reason.webSocketReadyState !== undefined) extra.webSocketReadyState = reason.webSocketReadyState;
        if (reason.messagePreview) extra.messagePreview = reason.messagePreview;
      }

      const postMessage = (msg: any) => {
        if (!shared) {
          (self as any).postMessage(msg);
        } else {
          const somePort = Object.values(ports).flat()[0];
          if (somePort) somePort.postMessage(msg);
          else console.error('[Worker] No ports available to send rejection', msg);
        }
      };

      postMessage({
        op: 'error',
        data: [
          message,
          {
            sendToSentry: true,
            sentryData: {
              error: serializedError,
              tags: {
                source: 'webworker-unhandledrejection',
                module: 'StoreWebWorker',
                errorName: reason?.name || 'UnhandledRejection',
                ...(reason?.exchange ? { exchange: reason.exchange } : {}),
              },
              extra,
            },
          },
        ],
      });
    } catch (e) {
      console.error('[Worker] Failed to forward unhandled rejection:', e);
    }
  });

  if (!shared) {
    // non shared start
    start(self as any);
  } else {
    // shared
    // @ts-expect-error - SharedWorkerGlobalScope.onconnect is not in standard Worker types
    self.onconnect = function (e) {
      // the incoming port
      const port = e.ports[0];
      port.start();
      start(port, true);
    };
  }
}

export default start;
