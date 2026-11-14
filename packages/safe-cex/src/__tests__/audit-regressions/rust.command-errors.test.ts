vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

// @vitest-environment node
import type { Transport, VenueErrorDetail, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { ExchangeError, nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { isErrorReported, markErrorReported, serializeCommandError } from '../../commandOutcome';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { setTelemetrySink } from '../../telemetry';
import { LogSeverity, OrderSide, OrderStatus, OrderType } from '../../types';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';
import { installNativeStartupFrames } from './rust.native-startup.fixtures';

type NativeCommand = { op: string; command?: number; [field: string]: unknown };
type Action = { op: string; [field: string]: unknown };
const accounts: RustEngineExchange[] = [];
const order = {
  symbol: 'BTCUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 1,
  price: 100,
  clientOrderId: 'requested',
};

afterEach(() => {
  accounts.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
  vi.useRealTimers();
  vi.restoreAllMocks();
  setTelemetrySink({});
});

async function fixture(name: ExchangeName = 'bybit', options: Partial<ExchangeOptions> = {}) {
  vi.useFakeTimers();
  vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(console, 'trace').mockImplementation(() => {});
  vi.spyOn(console, 'warn').mockImplementation(() => {});
  let handler = (command: NativeCommand): Action[] =>
    command.command === undefined ? [] : [{ op: 'done', command: command.command, result: '[]' }];
  let inbound: Action[] = [];
  let socket!: Parameters<Transport['connect']>[1];
  const commands: NativeCommand[] = [];
  const idle = () => '[]';
  const native: WasmEngineLike = {
    capabilities: () => engineCapabilities(name),
    start: () => '[{"op":"wsConnect","socket":7,"url":"wss://fixture.test"}]',
    wsOpen: idle,
    wsClose: idle, wsRouteExhausted: idle,
    httpStart: idle,
    httpChunk: idle,
    httpEnd: idle,
    httpFailed: idle,
    timer: idle,
    wsMessage: () => JSON.stringify(inbound.splice(0)),
    command(json) {
      const command: NativeCommand = JSON.parse(json);
      commands.push(command);
      return JSON.stringify(handler(command));
    },
    takeFrame: () => undefined,
    snapshotFrame: () => new Uint8Array(consumerFrame({ markets: [], tickers: [] })),
  };
  const store = new DefaultStore();
  // Startup readiness needs decoded native market/ticker sections, not bare STATUS flags.
  installNativeStartupFrames(native, store, () => ({
    loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
  }));
  setWasmEngineLoader(async () => ({
    WasmEngine: function () {
      return native;
    } as unknown as WasmEngineModule['WasmEngine'],
  }));
  setRustEngineTransport(() => ({
    now: Date.now,
    connect(_url, callbacks) {
      socket = callbacks;
      return { send() {}, close() {} };
    },
    async request() {
      throw new Error('Controlled ABI must not perform HTTP');
    },
    setTimeout,
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  }));
  const exchange = new RustEngineExchange(
    name,
    { key: '', secret: '', publicDataOnly: true, tuning: { consoleLogging: false }, ...options },
    store,
  );
  accounts.push(exchange);
  const errors = vi.fn();
  const logs = vi.fn();
  exchange.on('error', errors);
  exchange.on('log', logs);
  const startup = exchange.start();
  await vi.advanceTimersByTimeAsync(250);
  await startup;
  errors.mockClear();
  logs.mockClear();
  return {
    exchange,
    errors,
    logs,
    commands,
    onCommand: (next: typeof handler) => {
      handler = next;
    },
    deliver: (actions: Action[]) => {
      inbound = actions;
      socket.onMessage('fixture-action');
    },
  };
}

it('reports pure native DoneErr once and preserves the original host rejection and receipt fields', async () => {
  const r = await fixture();
  const reporting = vi.spyOn(r.exchange, 'emitAndLogError');
  r.onCommand(({ command }) => [{ op: 'done', command, error: 'venue rejected' }]);
  const error = await r.exchange.placeOrders([order]).catch((error: unknown) => error);
  expect(error).toBeInstanceOf(Error);
  expect(reporting).toHaveBeenCalledTimes(1);
  expect(reporting.mock.calls[0][0]).toBe(error);
  expect(error).toMatchObject({
    message: 'venue rejected',
    errorReported: true,
    commandId: 1,
    orderIdentities: [],
    actualClientOrderIds: [],
  });
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(serializeCommandError(error)).toMatchObject({ message: 'venue rejected', errorReported: true, commandId: 1 });
});

it('retains applied cancel batch counts and the unknown index without created order IDs', async () => {
  const r = await fixture();
  const detail: VenueErrorDetail = {
    message: 'Cancellation acknowledgement lost',
    kind: 'internal',
    outcome: 'unknown',
    partial: { orderIds: [], total: 3, failedIndex: 1 },
    elements: [
      { index: 0, status: 'applied', effect: 'cancel', targetOrderId: 'canceled' },
      { index: 1, status: 'unknown', error: { message: 'Acknowledgement lost', outcome: 'unknown' } },
      { index: 2, status: 'notSent' },
    ],
  };
  r.onCommand(({ command }) => [{ op: 'done', command, error: detail.message, detail }]);
  const targets = ['canceled', 'uncertain', 'untouched'].map((id) => ({
    ...order, id, status: OrderStatus.Open, filled: 0, remaining: 1, reduceOnly: false, lastUpdate: 0,
  }));
  const error = await r.exchange.cancelOrders(targets).catch((error: unknown) => error);
  expect(error).toBeInstanceOf(ExchangeError);
  const expected = {
    message: detail.message,
    kind: 'internal',
    outcome: 'unknown',
    elements: detail.elements,
    partialOrderIds: [],
    partialOrderCount: 0,
    totalOrderCount: 3,
    failedOrderIndex: 1,
  };
  expect(error).toMatchObject(expected);
  expect(serializeCommandError(error)).toMatchObject(expected);
  expect(r.errors).toHaveBeenCalledTimes(1);
});

it.each(['placement', 'cancellation'] as const)('retains BloFin %s facts and attempted counts at the JS boundary after Unknown', async (intent) => {
  const r = await fixture('blofin');
  const message = 'The outcome is unknown. Check Open Orders';
  const orderIds = intent === 'placement' ? ['accepted'] : [];
  const detail: VenueErrorDetail = {
    message,
    kind: 'internal',
    outcome: 'unknown',
    partial: { orderIds, total: 3, failedIndex: 1 },
    elements: [
      intent === 'placement'
        ? { index: 0, status: 'placed', orderIds: ['accepted'] }
        : { index: 0, status: 'applied', effect: 'cancel', targetOrderId: 'accepted-cancel' },
      { index: 1, status: 'unknown', error: { message, outcome: 'unknown' } },
      { index: 2, status: 'notSent' },
    ],
  };
  r.onCommand(({ command }) => [{ op: 'done', command, error: message, detail }]);
  const result = intent === 'placement'
    ? r.exchange.placeOrders(['first', 'second', 'third'].map((clientOrderId) => ({ ...order, clientOrderId })))
    : r.exchange.cancelOrders(['accepted-cancel', 'unknown-cancel', 'untouched'].map((id) => ({
      ...order, id, status: OrderStatus.Open, filled: 0, remaining: 1, reduceOnly: false, lastUpdate: 0,
    })));
  const error = await result.catch((error: unknown) => error);
  const expected = {
    message,
    outcome: 'unknown',
    elements: detail.elements,
    partialOrderIds: orderIds,
    partialOrderCount: orderIds.length,
    totalOrderCount: 3,
    failedOrderIndex: 1,
  };
  expect(error).toBeInstanceOf(ExchangeError);
  expect(error).toMatchObject(expected);
  expect(serializeCommandError(error)).toMatchObject(expected);
  expect(r.errors).toHaveBeenCalledTimes(1);
});

it.each([1, 0, 2])('retains Hyperliquid applied cancellation counts with an Unknown row at index %s', async (failedIndex) => {
  const r = await fixture('hyperliquid');
  const detail: VenueErrorDetail = {
    message: 'unclassified cancel result',
    kind: 'rejected',
    outcome: 'unknown',
    partial: { orderIds: [], total: 3, failedIndex },
  };
  r.onCommand(({ command }) => [{ op: 'done', command, error: detail.message, detail }]);
  const targets = ['9', '10', '11'].map((id) => ({
    ...order, id, symbol: 'BTC-PERP', status: OrderStatus.Open,
    filled: 0, remaining: 1, reduceOnly: false, lastUpdate: 0,
  }));
  const error = await r.exchange.cancelOrders(targets).catch((error: unknown) => error);
  expect(error).toBeInstanceOf(ExchangeError);
  // Zero counts created IDs; the batch still attempted all three cancellations.
  const expected = {
    message: detail.message,
    kind: 'rejected',
    outcome: 'unknown',
    partialOrderIds: [],
    partialOrderCount: 0,
    totalOrderCount: targets.length,
    failedOrderIndex: failedIndex,
  };
  expect(error).toMatchObject(expected);
  expect(serializeCommandError(error)).toMatchObject(expected);
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(r.commands.filter(({ op }) => op === 'cancelOrders')).toHaveLength(1);
});

it.each([false, true])('Bitunix applied cancel Unknown retains caller order and effects (unknownFirst=%s)', async (unknownFirst) => {
  const ids = unknownFirst ? ['uncertain', 'confirmed'] : ['confirmed', 'uncertain'];
  const failedIndex = unknownFirst ? 0 : 1;
  const r = await fixture('bitunix');
  const detail: VenueErrorDetail = {
    message: 'Bitunix cancellation not acknowledged for every order; replacement was not placed; The outcome is unknown. Check Open Orders',
    kind: 'network',
    outcome: 'unknown',
    venueCode: '10001',
    venueMessage: 'Network Error',
    partial: { orderIds: [], total: 2, failedIndex },
    elements: ids.map((id, index) => id === 'confirmed'
      ? { index, status: 'applied' as const, effect: 'cancel' as const, targetOrderId: id }
      : { index, status: 'unknown' as const, error: {
        message: 'Network Error', kind: 'network' as const, outcome: 'unknown' as const,
        venueCode: '10001', venueMessage: 'Network Error',
      } }),
  };
  r.onCommand(({ command }) => [{ op: 'done', command, error: detail.message, detail }]);
  const targets = ids.map((id) => ({
    ...order, id, status: OrderStatus.Open, filled: 0, remaining: 1, reduceOnly: false, lastUpdate: 0,
  }));
  const error = await r.exchange.cancelOrders(targets).catch((error: unknown) => error);
  expect(error).toBeInstanceOf(ExchangeError);
  expect(r.commands.filter(({ command }) => command !== undefined)).toEqual([
    expect.objectContaining({ op: 'cancelOrders', ids }),
  ]);
  const expected = {
    message: detail.message,
    kind: 'network',
    outcome: 'unknown',
    venueCode: '10001',
    venueMessage: 'Network Error',
    partialOrderIds: [],
    // This counts created IDs, not confirmed cancellation effects.
    partialOrderCount: 0,
    totalOrderCount: 2,
    failedOrderIndex: failedIndex,
    elements: detail.elements,
    errorReported: true,
  };
  expect(error).toMatchObject(expected);
  expect(serializeCommandError(error)).toMatchObject(expected);
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(r.errors.mock.calls[0][0]).toBe(detail.message);
  expect(r.errors.mock.calls[0][1].classified).toEqual({ kind: 'network', outcome: 'unknown', venueCode: '10001' });
});

it('preserves Bybit applied cancel counts when one target is canceled and the other is unknown', async () => {
  const r = await fixture('bybit');
  const detail: VenueErrorDetail = {
    message: 'The outcome is unknown. Check Open Orders',
    kind: 'venueUnavailable',
    outcome: 'unknown',
    venueCode: '10016',
    venueMessage: 'unknown cancellation outcome',
    partial: { orderIds: [], total: 2, failedIndex: 1 },
    elements: [
      { index: 0, status: 'applied', effect: 'cancel', targetOrderId: 'one' },
      { index: 1, status: 'unknown', error: {
        message: 'The outcome is unknown. Check Open Orders', kind: 'venueUnavailable',
        outcome: 'unknown', venueCode: '10016', venueMessage: 'unknown cancellation outcome',
      } },
    ],
  };
  // Matches trading_variants::applied_batch_cancel_unknown_row_records_only_confirmed_cancellations.
  // Zero counts created IDs; the native test proves target "one" was canceled.
  r.onCommand(({ command }) => [{ op: 'done', command, error: detail.message, detail }]);
  const targets = ['one', 'two'].map((id) => ({
    ...order, id, status: OrderStatus.Open, filled: 0, remaining: 1, reduceOnly: false, lastUpdate: 0,
  }));
  const error = await r.exchange.cancelOrders(targets).catch((error: unknown) => error);
  const expected = {
    message: detail.message,
    outcome: 'unknown',
    elements: detail.elements,
    partialOrderIds: [],
    partialOrderCount: 0,
    totalOrderCount: 2,
    failedOrderIndex: 1,
  };
  expect(error).toBeInstanceOf(ExchangeError);
  expect(error).toMatchObject(expected);
  expect(serializeCommandError(error)).toMatchObject(expected);
  expect(r.commands.filter(({ op }) => op === 'cancelOrders')).toMatchObject([
    { ids: ['one', 'two'], orders: targets.map(({ id, symbol, type }) => ({ id, symbol, type })) },
  ]);
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(r.errors.mock.calls[0][0]).toBe(detail.message);
  expect(r.errors.mock.calls[0][1]).toMatchObject({ classified: { outcome: 'unknown' } });
});

it('keeps populated placement error output byte-identical to master 343e38cbe96', () => {
  const detail: VenueErrorDetail = {
    message: 'Second placement acknowledgement lost',
    kind: 'internal',
    outcome: 'unknown',
    venueCode: '50004',
    partial: { orderIds: ['accepted'], total: 2, failedIndex: 1 },
    elements: [
      { index: 0, status: 'placed', orderIds: ['accepted'] },
      { index: 1, status: 'unknown', error: { message: 'Acknowledgement lost', outcome: 'unknown' } },
    ],
  };
  const error = new ExchangeError(detail.message, detail);
  // Captured and passed on master before changing the host mapper.
  expect(JSON.stringify(error)).toBe(
    '{"kind":"internal","outcome":"unknown","venueCode":"50004","elements":[{"index":0,"status":"placed","orderIds":["accepted"]},{"index":1,"status":"unknown","error":{"message":"Acknowledgement lost","outcome":"unknown"}}],"partialOrderIds":["accepted"],"partialOrderCount":1,"totalOrderCount":2,"failedOrderIndex":1}',
  );
  expect(error.message).toBe(detail.message);
  expect(String(error)).toBe(`Error: ${detail.message}`);
  expect(error.partialOrderIds).not.toBe(detail.partial!.orderIds);
});

it.each([false, true])('reports attributed native diagnostics and DoneErr once (deferred=%s)', async (deferred) => {
  const r = await fixture();
  let id!: number;
  r.onCommand(({ command }) => {
    id = command!;
    return [
      { op: 'commandError', command, message: 'same rejection' },
      ...(!deferred ? [{ op: 'done', command, error: 'same rejection' }] : []),
    ];
  });
  const failure = r.exchange.placeOrders([order]).catch((error: unknown) => error);
  if (deferred) {
    expect(r.errors).not.toHaveBeenCalled();
    r.deliver([{ op: 'done', command: id, error: 'same rejection' }]);
  }
  expect(await failure).toMatchObject({ message: 'same rejection', errorReported: true });
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(r.logs).toHaveBeenCalledWith('[bybit] same rejection', LogSeverity.Warning);
});

it('preserves known partial success while reporting the first associated diagnostic once', async () => {
  const r = await fixture();
  r.onCommand(({ command }) => [
    { op: 'commandError', command, message: 'first refused child' },
    { op: 'commandError', command, message: 'second refused child' },
    { op: 'done', command, result: '["accepted"]' },
  ]);
  await expect(r.exchange.placeOrders([order])).resolves.toEqual(['accepted']);
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(r.errors.mock.calls[0][0]).toContain('first refused child');
  expect(r.logs.mock.calls.filter(([, severity]) => severity === LogSeverity.Warning)).toHaveLength(2);
});

it('reports independent identical failures independently and preserves ordinary background logs', async () => {
  const r = await fixture();
  r.onCommand(({ command }) => [{ op: 'done', command, error: 'identical' }]);
  const first = await r.exchange.placeOrders([order]).catch((error: unknown) => error);
  const second = await r.exchange.placeOrders([order]).catch((error: unknown) => error);
  expect(first).not.toBe(second);
  expect(r.errors).toHaveBeenCalledTimes(2);
  r.deliver([
    { op: 'event', code: 'test.diagnostic', level: 'error', message: 'identical' },
    { op: 'event', code: 'test.diagnostic', level: 'info', message: 'background info' },
  ]);
  expect(r.errors).toHaveBeenCalledTimes(3);
  expect(r.logs).toHaveBeenCalledWith('[bybit] background info', LogSeverity.Info, {
    code: 'test.diagnostic', level: 'info', message: 'background info', fields: {},
  });
});

it('reports a timeout once while later attributed errors and DoneErr remain diagnostic logs', async () => {
  const r = await fixture('bybit', { extra: { rustCommandTimeoutMs: 25 } });
  let id!: number;
  r.onCommand(({ command }) => {
    id = command!;
    return [];
  });
  const failure = r.exchange.placeOrders([order]).catch((error: unknown) => error);
  await vi.advanceTimersByTimeAsync(25);
  expect(await failure).toMatchObject({
    code: 'COMMAND_TIMEOUT',
    outcome: 'unknown',
    clientOrderId: 'requested',
    errorReported: true,
  });
  expect(r.errors).toHaveBeenCalledTimes(1);
  r.deliver([
    { op: 'commandError', command: id, message: 'late venue error' },
    { op: 'done', command: id, error: 'late venue error' },
  ]);
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(r.logs).toHaveBeenCalledWith('[bybit] late venue error', LogSeverity.Warning);
});

it('owns a replyless subscription rejection without a second subscription catch report', async () => {
  const r = await fixture();
  r.onCommand(({ op }) => {
    if (op === 'setTickerSymbols') throw new Error('invalid subscription');
    return [];
  });
  r.exchange.setActiveTickerSymbols(['BTCUSDT']);
  await vi.advanceTimersByTimeAsync(0);
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(r.errors.mock.calls[0][0]).toContain('invalid subscription');
  expect(r.exchange.isDisposed).toBe(false);
});

it('retains partial results, actual identity namespaces and original Error when the error listener throws', async () => {
  const r = await fixture();
  r.exchange.on('error', () => {
    throw new Error('toast consumer failed');
  });
  const partial = {
    message: 'second refused',
    partialOrderIds: ['accepted'],
    partialOrderCount: 1,
    totalOrderCount: 2,
    failedOrderIndex: 1,
  };
  r.onCommand(({ command }) => [
    {
      op: 'commandProgress',
      command,
      receipts: [
        { symbol: 'BTCUSDT', kind: 'clientOrderId', identity: 'actual' },
        { symbol: 'BTCUSDT', kind: 'clientOrderIndex', identity: '9007199254740993' },
      ],
    },
    { op: 'done', command, error: JSON.stringify(partial) },
  ]);
  const error = await r.exchange.placeOrders([order]).catch((error: unknown) => error);
  expect(error).toMatchObject({ ...partial, actualClientOrderIds: ['actual'], errorReported: true });
  expect(error).not.toHaveProperty('outcome');
  expect(serializeCommandError(error)).toMatchObject({
    ...partial,
    errorReported: true,
    orderIdentities: [
      { symbol: 'BTCUSDT', kind: 'clientOrderId', identity: 'actual' },
      { symbol: 'BTCUSDT', kind: 'clientOrderIndex', identity: '9007199254740993' },
    ],
  });
  expect(r.errors).toHaveBeenCalledTimes(1);
});

it('keeps a warning-listener failure diagnostic and emits only the command rejection error', async () => {
  const r = await fixture();
  r.exchange.on('log', () => {
    throw new Error('log consumer failed');
  });
  r.onCommand(({ command }) => [
    { op: 'commandError', command, message: 'native failure' },
    { op: 'done', command, error: 'native failure' },
  ]);
  await expect(r.exchange.placeOrders([order])).rejects.toThrow('native failure');
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(r.exchange.isEngineDead).toBe(false);
});

it('preserves a frozen Error and its unknown outcome when both reporting and native metadata mutation fail', async () => {
  const r = await fixture();
  const original = Object.freeze(
    Object.assign(new Error('write interrupted'), {
      code: 'ENGINE_PROTOCOL_ERROR',
      outcome: 'unknown',
      clientOrderIds: ['caller'],
      actualClientOrderIds: ['actual'],
      partialOrderIds: ['accepted'],
    }),
  );
  r.onCommand(() => {
    throw original;
  });
  r.exchange.on('error', () => {
    throw new Error('toast failed');
  });
  const failure = await r.exchange.placeOrders([order]).catch((error: unknown) => error);
  expect(failure).toBe(original);
  expect(isErrorReported(failure)).toBe(true);
  expect(serializeCommandError(failure)).toMatchObject({
    errorReported: true,
    code: 'ENGINE_PROTOCOL_ERROR',
    outcome: 'unknown',
    clientOrderIds: ['caller'],
    actualClientOrderIds: ['actual'],
    partialOrderIds: ['accepted'],
  });
  expect(r.errors).toHaveBeenCalledTimes(1);
});

it('preserves a non-Error rejection even when error reporting itself throws', async () => {
  const r = await fixture();
  r.onCommand(() => {
    throw 'native rejection';
  });
  vi.spyOn(r.exchange, 'emitAndLogError').mockImplementation(() => {
    throw new Error('reporter failed');
  });
  expect(await r.exchange.placeOrders([order]).catch((error: unknown) => error)).toBe('native rejection');
});

it('does not report an already reported Error or a terminal-engine interruption again', async () => {
  const r = await fixture();
  const original = new Error('already reported');
  markErrorReported(original);
  r.onCommand(() => {
    throw original;
  });
  expect(await r.exchange.placeOrders([order]).catch((error: unknown) => error)).toBe(original);
  expect(r.errors).not.toHaveBeenCalled();
  r.onCommand(() => {
    throw new WebAssembly.RuntimeError('unreachable');
  });
  const error = await r.exchange.placeOrders([order]).catch((error: unknown) => error);
  expect(error).toMatchObject({ code: 'ENGINE_DEAD', outcome: 'unknown', errorReported: true });
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(r.exchange.isEngineDead).toBe(true);
});

it('reports command-local validation but does not wrap existing public preflight reporting twice', async () => {
  const r = await fixture('coinbase');
  await expect(r.exchange.setLeverage('BTCUSDT', NaN)).rejects.toThrow('Coinbase command numbers must be finite');
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(r.commands.filter(({ op }) => op === 'setLeverage')).toHaveLength(0);
  await expect(r.exchange.placeOrders([{ ...order, impliedVolatility: 0.5 }])).rejects.toThrow(
    'Options are not enabled',
  );
  expect(r.errors).toHaveBeenCalledTimes(2);
});

it('exposes native codes and fields while preserving legacy Error and Warn delivery', async () => {
  const r = await fixture();
  const events = vi.fn();
  const captureException = vi.fn();
  setTelemetrySink({ captureException });
  r.exchange.on('engine-event', events);
  const auth = { level: 'error', code: 'auth.rejected', message: 'new refusal wording', fields: { source: 'private' } };
  const retry = { level: 'warn', code: 'request.failed', message: 'retrying', fields: { attempt: 2 } };
  const debug = { level: 'debug', code: 'ws.closed', message: 'closed', fields: {} };
  r.deliver([auth, retry, debug].map(event => ({ op: 'event', ...event })));
  expect(events.mock.calls).toEqual([[auth], [retry], [debug]]);
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(captureException.mock.calls[0][1]).toMatchObject({ tags: { event_code: 'auth.rejected' }, extra: { engineEvent: auth } });
  expect(captureException.mock.calls[0][1].tags.engineEvent).toBeUndefined();
  expect(r.errors.mock.calls[0][0]).toContain(auth.message);
  expect(r.errors.mock.calls[0][1]).toMatchObject({ engineEvent: auth });
  expect(r.logs).toHaveBeenCalledExactlyOnceWith('[bybit] retrying', LogSeverity.Warning, retry);
});

it('reports a typed GRVT builder rejection once while preserving actionable approval fields', async () => {
  const r = await fixture('grvt');
  const reporting = vi.spyOn(r.exchange, 'emitAndLogError');
  const builderAuthorization = {
    code: 'GRVT_BUILDER_UNAUTHORIZED',
    reason: 'cap-too-low',
    requiredRaw: 300,
    authorizedRaw: 100,
  };
  r.onCommand(({ command }) => [
    {
      op: 'done',
      command,
      error: 'Builder cap too low',
      detail: {
        message: 'Builder cap too low',
        kind: 'invalidInput',
        outcome: 'notSent',
        builderAuthorization,
      },
    },
  ]);
  const error = await r.exchange.placeOrders([{ ...order, clientOrderId: '42' }]).catch((error: unknown) => error);
  expect(error).toMatchObject({ ...builderAuthorization, kind: 'invalidInput', outcome: 'notSent' });
  expect(reporting).toHaveBeenCalledTimes(1);
  expect(r.errors).toHaveBeenCalledTimes(1);
  expect(serializeCommandError(error)).toMatchObject({ ...builderAuthorization, errorReported: true });
});

it('keeps an unsupported history read quiet but still rejects it, typed', async () => {
  const r = await fixture('bybit', { key: 'fixture-key', secret: 'fixture-secret', publicDataOnly: false });
  r.onCommand(({ command }) => [
    {
      op: 'done',
      command,
      error: 'MEXC income supports only kind FUNDING_FEE',
      detail: { message: 'MEXC income supports only kind FUNDING_FEE', kind: 'unsupported', outcome: 'notSent' },
    },
  ]);
  const error = await r.exchange.fetchIncomeHistory({ incomeType: 'COMMISSION' }).catch((error: unknown) => error);
  expect(error).toMatchObject({ kind: 'unsupported', message: 'MEXC income supports only kind FUNDING_FEE' });
  expect(r.errors).not.toHaveBeenCalled();
  r.onCommand(({ command }) => [{ op: 'done', command, error: 'income read failed' }]);
  await r.exchange.fetchIncomeHistory({ incomeType: 'FUNDING_FEE' }).catch(() => undefined);
  expect(r.errors).toHaveBeenCalledTimes(1);
});

// Mirrors OKX's regular/algo recording pins: one accepted cancel and one 51412 row.
it.each([false, true])('OKX applied cancel keeps Unknown counts without created IDs (algo=%s)', async (algo) => {
  const r = await fixture('okx');
  const events = vi.fn();
  r.exchange.on('engine-event', events);
  const nativeIds = algo ? ['91', '92'] : ['1', '2'];
  const unknownMessage = `OKX cancellation target ${nativeIds[1]}: Cancellation timed out (code 51412). The outcome is unknown. Check Open Orders`;
  const detail: VenueErrorDetail = {
    message: 'Cancellation timed out (code 51412). The outcome is unknown. Check Open Orders',
    kind: 'timeout',
    outcome: 'unknown',
    venueCode: '51412',
    venueMessage: 'Cancellation timed out',
    partial: { orderIds: [], total: 2, failedIndex: 1 },
    elements: [
      { index: 0, status: 'applied', effect: 'cancel', targetOrderId: nativeIds[0] },
      {
        index: 1, status: 'unknown', error: {
          message: unknownMessage, kind: 'timeout', outcome: 'unknown',
          venueCode: '51412', venueMessage: 'Cancellation timed out',
        },
      },
    ],
  };
  r.onCommand(({ command }) => [
    {
      op: 'event', level: 'warn', code: 'order.outcome_unknown', message: unknownMessage,
      fields: { command, index: 1, targetOrderId: nativeIds[1], venueCode: '51412' },
    },
    { op: 'done', command, error: detail.message, detail },
  ]);
  const targets = (algo ? ['91_sl', '92_sl'] : ['1', '2']).map((id) => ({
    ...order, id, type: algo ? OrderType.StopLoss : OrderType.Limit,
    status: OrderStatus.Open, filled: 0, remaining: 1, reduceOnly: false, lastUpdate: 0,
  }));
  const error = await r.exchange.cancelOrders(targets).catch((error: unknown) => error);
  expect(error).toBeInstanceOf(ExchangeError);
  expect(r.commands.findLast(({ op }) => op.startsWith('cancelOrders'))?.orders).toEqual(
    targets.map(({ id, symbol, type }) => ({ id, symbol, type })),
  );
  const expected = {
    message: detail.message,
    outcome: 'unknown',
    kind: 'timeout',
    venueCode: '51412',
    partialOrderIds: [],
    partialOrderCount: 0,
    totalOrderCount: 2,
    failedOrderIndex: 1,
    elements: detail.elements,
  };
  // The zero counts created IDs; the attempted cancellation remains uncertain.
  expect(error).toMatchObject(expected);
  expect(serializeCommandError(error)).toMatchObject(expected);
  const unknownEvents = events.mock.calls.filter(([event]) => event.code === 'order.outcome_unknown');
  expect(unknownEvents).toEqual([[expect.objectContaining({
    level: 'warn', code: 'order.outcome_unknown', message: unknownMessage,
    fields: expect.objectContaining({ index: 1, targetOrderId: nativeIds[1], venueCode: '51412' }),
  })]]);
  expect(r.errors).toHaveBeenCalledTimes(1);
});
