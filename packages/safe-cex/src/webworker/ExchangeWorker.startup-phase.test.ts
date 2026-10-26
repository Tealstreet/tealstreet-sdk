import { describe, expect, it, vi } from 'vitest';

import { DefaultStore } from '../store/store.base';

/**
 * Drives the real `start()` failure path rather than calling `logError`
 * directly. A plumbing test on `logError` passes whether or not the CALL SITE
 * passes the phase, which is the whole change — this is the one that goes red
 * if the tag is dropped from `start()`'s catch.
 *
 * The registry mock is also what exercises the PRE-exchange failures (unknown
 * venue, a rejecting dynamic `loader()` — i.e. a chunk-load error). Those used
 * to reject out of `start()` entirely and land in the global handler untagged.
 */

const loaderMock = vi.fn();
const startMock = vi.fn();
const onMock = vi.fn();

class FakeExchange {
  store: DefaultStore;
  isDisposed = false;
  start = startMock;
  on = onMock;
  disablePublicWebsockets = vi.fn();

  // Mirrors the real signature: the worker hands its own store to the exchange,
  // and `_setupExchangeEventListeners` subscribes to `exchange.store`. Creating
  // a fresh store here would subscribe to one the worker never writes to.
  constructor(_name: string, _options: unknown, store: DefaultStore) {
    this.store = store;
  }
}

vi.mock('../utils/virtualClock', () => ({
  virtualClock: { start: vi.fn(async () => undefined) },
}));

vi.mock('../exchanges/registry', () => ({
  exchangeRegistry: {
    get bybit() {
      return loaderMock;
    },
  },
}));

const { ExchangeWorker } = await import('./ExchangeWorker');

const createWorker = (exchangeName = 'bybit') => {
  const messages: any[] = [];
  const ctx = { postMessage: vi.fn((message: any) => messages.push(message)) };
  const worker = new ExchangeWorker(
    ctx as any,
    'bybit-main',
    exchangeName as any,
    {} as any,
    {} as any,
    {} as any,
    'session',
  );
  return { messages, worker };
};

const errorPayloads = (messages: any[]) => messages.filter((m) => m.op === 'error');

const phaseOf = (messages: any[]) => errorPayloads(messages)[0]?.data[1].sentryData.tags.worker_error_phase;

describe('ExchangeWorker startup phase tagging', () => {
  it('tags a venue start() rejection with worker_error_phase', async () => {
    loaderMock.mockResolvedValue(FakeExchange);
    startMock.mockRejectedValue(new Error('markets fetch blew up'));
    const { messages, worker } = createWorker();

    await expect(worker.start()).resolves.toBe(false);

    expect(errorPayloads(messages)).toHaveLength(1);
    expect(phaseOf(messages)).toBe('startup');
  });

  // The venue's own message is the only thing the web-side family classifier
  // can see; "Error starting exchange" lives in the log message arg. Both must
  // survive together or the phase cannot be cross-referenced with the cause.
  it('keeps the venue failure message alongside the phase tag', async () => {
    loaderMock.mockResolvedValue(FakeExchange);
    startMock.mockRejectedValue(new Error('markets fetch blew up'));
    const { messages, worker } = createWorker();

    await worker.start();

    const error = errorPayloads(messages)[0];
    expect(error.data[0]).toContain('Error starting exchange: bybit');
    expect(error.data[1].sentryData.error.message).toBe('markets fetch blew up');
  });

  // Pre-exchange failures. These rejected out of `start()` before the try was
  // widened, so they reached the global `unhandledrejection` handler untagged
  // AND skipped the caller's `!started` cleanup.
  it('tags a chunk-load failure from the dynamic exchange loader', async () => {
    loaderMock.mockRejectedValue(new Error('Failed to fetch dynamically imported module'));
    const { messages, worker } = createWorker();

    await expect(worker.start()).resolves.toBe(false);

    expect(phaseOf(messages)).toBe('startup');
  });

  it('returns false instead of throwing on an unknown exchange', async () => {
    const { messages, worker } = createWorker('not-a-venue');

    await expect(worker.start()).resolves.toBe(false);

    expect(phaseOf(messages)).toBe('startup');
    expect(errorPayloads(messages)[0].data[0]).toContain('not-a-venue is unsupported in this build');
  });

  it('does not tag a successful startup', async () => {
    loaderMock.mockResolvedValue(FakeExchange);
    startMock.mockResolvedValue(undefined);
    const { messages, worker } = createWorker();

    await worker.start();

    expect(errorPayloads(messages)).toHaveLength(0);
  });
});

it('forwards coded diagnostics and legacy metadata only from the current worker generation', async () => {
  loaderMock.mockResolvedValue(FakeExchange);
  startMock.mockResolvedValue(undefined);
  onMock.mockClear();
  const { messages, worker } = createWorker();
  await worker.start();
  const event = { code: 'auth.rejected', level: 'error', message: 'new refusal', fields: { source: 'private' } };
  const listener = onMock.mock.calls.find(([name]) => name === 'engine-event')?.[1];
  expect(listener).toBeTypeOf('function');
  listener(event);
  expect(messages.filter((message) => message.op === 'engine-event')).toEqual([{ op: 'engine-event', data: [event] }]);
  const errorListener = onMock.mock.calls.find(([name]) => name === 'error')![1];
  errorListener(event.message, { engineEvent: event });
  expect(errorPayloads(messages).at(-1).data).toEqual([event.message, { engineEvent: event }]);
  // A restart invalidates callbacks held by the old exchange.
  await worker.start();
  listener(event);
  expect(messages.filter((message) => message.op === 'engine-event')).toHaveLength(1);
});
