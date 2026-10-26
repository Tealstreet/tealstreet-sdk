vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

// @vitest-environment node
import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { isErrorReported, markErrorReported, serializeCommandError } from '../../commandOutcome';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { setTelemetrySink } from '../../telemetry';
import { LogSeverity, OrderSide, OrderType } from '../../types';
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
