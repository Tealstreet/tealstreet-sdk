import { expect, it, vi } from 'vitest';

/**
 * A worker-side Rust auth rejection must leave the worker as main-thread events:
 * the coded `engine-event` and an `'error'` whose `classified` carries the
 * engine's own kind and venue code, so the host toasts it and its health
 * classification can latch the account's auth state. Silent zeros on a live
 * Bybit account (2026-10-30) were a private-API rejection that never surfaced.
 */

vi.mock('../../utils/virtualClock', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: vi.fn(async () => undefined) } };
});

const { RustEngineExchange } = await import('../../exchanges/rust/rust.exchange');

/** The real adapter, never started: only its event boundary is exercised. */
class UnstartedRust extends RustEngineExchange {
  override start = async () => {};
}

vi.mock('../../exchanges/registry', () => ({
  exchangeRegistry: {
    get bybit() {
      return async () => UnstartedRust;
    },
  },
}));

const { ExchangeWorker } = await import('../../webworker/ExchangeWorker');

const startWorker = async () => {
  const messages: any[] = [];
  const ctx = { postMessage: vi.fn((message: any) => messages.push(message)) };
  const worker = new ExchangeWorker(ctx as any, 'bybit-main', 'bybit', {} as any, {} as any, {} as any, 'session');
  await worker.start();
  const exchange = (worker as unknown as { _exchange: InstanceType<typeof RustEngineExchange> })._exchange;
  expect(exchange).toBeInstanceOf(RustEngineExchange);
  return { messages, exchange, receive: (exchange as any).receiveEvent as (event: unknown) => void };
};

it('a worker-side auth rejection posts the engine event and a classified error to the main thread', async () => {
  const { messages, exchange, receive } = await startWorker();
  const event = {
    level: 'error',
    code: 'auth.rejected',
    message: 'Bybit rejected the API key (10010): Unmatched IP, please check your API key bound IP addresses.',
    fields: { kind: 'authPermission', venueCode: '10010' },
  };
  receive(event);

  expect(messages.filter((message) => message.op === 'engine-event')).toEqual([{ op: 'engine-event', data: [event] }]);
  const errors = messages.filter((message) => message.op === 'error');
  expect(errors).toHaveLength(1);
  const [text, options] = errors[0].data;
  expect(text).toContain('Bybit rejected the API key (10010)');
  expect(options.classified).toEqual({ kind: 'authPermission', venueCode: '10010' });
  expect(options.engineEvent).toEqual(event);
  // The worker's own polling backs off on the rejected key.
  expect(exchange.authFailureStreak).toBe(1);
});

it('a read refusal stays an error without asserting an account auth kind', async () => {
  const { messages, exchange, receive } = await startWorker();
  receive({
    level: 'error',
    code: 'request.failed',
    message: 'Bybit balance: API key is invalid.',
    fields: { kind: 'authInvalid', venueCode: '10003' },
  });

  const [error] = messages.filter((message) => message.op === 'error');
  expect(error.data[1].classified).toEqual({ venueCode: '10003' });
  expect(exchange.authFailureStreak).toBe(0);
});

it('warn-level engine events are forwarded but raise no error', async () => {
  const { messages, receive } = await startWorker();
  receive({ level: 'warn', code: 'request.failed', message: 'Bybit orders: timeout', fields: {} });

  expect(messages.filter((message) => message.op === 'engine-event')).toHaveLength(1);
  expect(messages.filter((message) => message.op === 'error')).toHaveLength(0);
});
