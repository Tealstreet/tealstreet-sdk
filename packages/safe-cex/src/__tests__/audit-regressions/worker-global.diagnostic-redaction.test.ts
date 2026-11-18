import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Keep the real worker entry, registration, initialization and global handlers.
// Stub only account construction so no exchange transport can start.
vi.mock('../../webworker/ExchangeWorker', () => ({
  ExchangeWorker: class {
    exchange = { isDisposed: false };
    store = { update: vi.fn() };
    start = vi.fn(async () => true);
    constructor(readonly context: unknown) {}
  },
}));
vi.mock('../../webworker/TelgramWorker', () => ({ TelegramWorker: class {} }));
vi.mock('../../utils/virtualClock', () => ({ virtualClock: { syncFromServerTime: vi.fn() } }));

type Listener = (event: any) => unknown;
const sentinel = 'synthetic-global-worker-material';

beforeEach(() => {
  vi.resetModules();
});
afterEach(() => {
  vi.unstubAllGlobals();
});

async function loadWorker(shared: boolean) {
  const listeners = new Map<string, Listener>();
  const scope = {
    ...(shared ? { SharedWorkerGlobalScope: class {} } : {}),
    addEventListener: (type: string, listener: Listener) => listeners.set(type, listener),
    postMessage: vi.fn(),
    onconnect: undefined as ((event: { ports: unknown[] }) => void) | undefined,
  };
  vi.stubGlobal('self', scope);
  await import('../../webworker/store.webworker');
  let postMessage = scope.postMessage;
  if (shared) {
    let onMessage: Listener | undefined;
    const port = {
      start: vi.fn(),
      postMessage: vi.fn(),
      addEventListener: (type: string, listener: Listener) => {
        if (type === 'message') onMessage = listener;
      },
    };
    scope.onconnect!({ ports: [port] });
    await onMessage!({
      data: {
        op: 'init',
        data: {
          accountName: 'fixture-account',
          exchangeName: 'bybit',
          options: {},
          settings: {},
          metadata: {},
          sessionKey: 'synthetic-session',
        },
      },
    });
    postMessage = port.postMessage;
    postMessage.mockClear();
  }
  return { listeners, postMessage };
}

describe.each([false, true])('registered global worker diagnostics (shared=%s)', (shared) => {
  it.each(['error', 'unhandledrejection'])('redacts the entire %s diagnostic before delivery', async (type) => {
    const { listeners, postMessage } = await loadWorker(shared);
    const error = Object.assign(new Error(`request failed apiKey="${sentinel}"`), {
      exchange: 'bybit',
      webSocketState: 'closed',
      webSocketReadyState: 3,
      messagePreview: JSON.stringify({ jwt: sentinel }),
      isReconnecting: false,
      originalError: new Error(`token="${sentinel}"`),
    });
    listeners.get(type)!({ error, reason: error });
    expect(postMessage).toHaveBeenCalledTimes(1);
    const packet = postMessage.mock.calls[0][0];
    expect(JSON.stringify(packet)).not.toContain(sentinel);
    expect(packet).toMatchObject({
      op: 'error',
      data: [
        expect.any(String),
        {
          sendToSentry: true,
          sentryData: {
            tags: { module: 'StoreWebWorker', exchange: 'bybit' },
            extra: { webSocketReadyState: 3 },
          },
        },
      ],
    });
    expect(error.message).toContain(sentinel);
    expect(error.messagePreview).toContain(sentinel);
  });

  it.each(['error', 'unhandledrejection'])('keeps ordinary %s context and delivery shape', async (type) => {
    const { listeners, postMessage } = await loadWorker(shared);
    const error = Object.assign(new Error('socket closed unexpectedly'), {
      exchange: 'bybit',
      webSocketState: 'closed',
      webSocketReadyState: 3,
      messagePreview: JSON.stringify({ op: 'subscribe', args: ['orders'], requestId: 'r-1', count: 2 }),
    });
    listeners.get(type)!({ error, reason: error });
    expect(postMessage).toHaveBeenCalledTimes(1);
    expect(postMessage.mock.calls[0][0]).toMatchObject({
      op: 'error',
      data: [
        'socket closed unexpectedly',
        {
          sendToSentry: true,
          sentryData: {
            error: { name: 'Error', message: error.message, stack: error.stack },
            tags: { source: type === 'error' ? 'webworker-uncaught' : 'webworker-unhandledrejection' },
            extra: { originalMessage: error.message, webSocketReadyState: 3, messagePreview: error.messagePreview },
          },
        },
      ],
    });
  });
});
