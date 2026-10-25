import type { ExchangeOptions } from '../../types';
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { afterEach, expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';

const accounts: BaseExchange[] = [];
const account = (rust = true, options: Partial<ExchangeOptions> = { publicDataOnly: true }): BaseExchange => {
  const config = { key: '', secret: '', ...options };
  const exchange = rust
    ? new RustEngineExchange('bybit', config, new DefaultStore())
    : new BaseExchange('bybit', config, new DefaultStore());
  accounts.push(exchange);
  return exchange;
};

afterEach(() => {
  accounts.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  vi.useRealTimers();
});

it('registers the response listener before dispatching a synchronous worker reply', async () => {
  const exchange = account();
  exchange.emitter.on('callMethod', ({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', { requestId, success: true, result: ['acknowledged'] });
  });
  await expect(exchange.callMethodInWorker('fetchOrders')).resolves.toEqual(['acknowledged']);
});

it('reconstructs unknown outcomes and client attribution at the main-thread consumer', async () => {
  const exchange = account();
  exchange.emitter.on('callMethod', ({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', {
      requestId,
      success: false,
      result: {
        message: 'Reconcile before retrying',
        name: 'CommandInterruptedError',
        code: 'ENGINE_DISPOSED',
        command: 'setLeverage',
        outcome: 'unknown',
        clientOrderId: 'actual-client',
        clientOrderIds: ['actual-client', 'child-client'],
        cause: { credentials: 'must not cross the error boundary' },
      },
    });
  });
  const failure = await exchange.callMethodInWorker('setLeverage', 'BTCUSDT', 3).catch((error: unknown) => error);
  expect(failure).toMatchObject({
    message: 'Reconcile before retrying', code: 'ENGINE_DISPOSED', command: 'setLeverage', outcome: 'unknown',
    clientOrderId: 'actual-client', clientOrderIds: ['actual-client', 'child-client'],
  });
  expect(failure).not.toHaveProperty('cause');
});

it('retains known partial rejections without converting them to unknown outcomes', async () => {
  const exchange = account();
  exchange.emitter.on('callMethod', ({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', {
      requestId, success: false,
      result: {
        message: 'Second order rejected', code: 400,
        partialOrderIds: ['accepted-id'], partialOrderCount: 1, totalOrderCount: 2, failedOrderIndex: 1,
        clientOrderIds: ['valid', 123],
      },
    });
  });
  const failure = await exchange.callMethodInWorker('fetchOrders').catch((error: unknown) => error);
  expect(failure).toMatchObject({
    message: 'Second order rejected', code: 400,
    partialOrderIds: ['accepted-id'], partialOrderCount: 1, totalOrderCount: 2, failedOrderIndex: 1,
  });
  expect(failure).not.toHaveProperty('outcome');
  expect(failure).not.toHaveProperty('clientOrderIds');
});

it('keeps the TS worker waiting deadline at five seconds', async () => {
  vi.useFakeTimers();
  const exchange = account(false);
  const failure = exchange.callMethodInWorker('fetchOrders').catch((error: unknown) => error);
  await vi.advanceTimersByTimeAsync(5000);
  await expect(failure).resolves.toMatchObject({ message: 'Timeout callMethodInWorker' });
  expect(vi.getTimerCount()).toBe(0);
});

it('waits for the Rust host deadline and reply delivery before declaring a worker unavailable', async () => {
  vi.useFakeTimers();
  const exchange = account();
  let settled = false;
  const failure = exchange.callMethodInWorker('setLeverage', 'BTCUSDT', 3).catch((error: unknown) => {
    settled = true;
    return error;
  });
  await vi.advanceTimersByTimeAsync(120_999);
  expect(settled).toBe(false);
  await vi.advanceTimersByTimeAsync(1);
  await expect(failure).resolves.toMatchObject({ code: 'COMMAND_TIMEOUT', command: 'setLeverage', outcome: 'unknown' });
  expect(vi.getTimerCount()).toBe(0);
});

it('uses the configured Rust deadline and keeps reads distinct from interrupted writes', async () => {
  vi.useFakeTimers();
  const exchange = account(true, { publicDataOnly: true, extra: { rustCommandTimeoutMs: 6000 } });
  let settled = false;
  const failure = exchange.callMethodInWorker('fetchOrders').catch((error: unknown) => {
    settled = true;
    return error;
  });
  await vi.advanceTimersByTimeAsync(6999);
  expect(settled).toBe(false);
  await vi.advanceTimersByTimeAsync(1);
  const error = await failure;
  expect(error).toMatchObject({ code: 'COMMAND_TIMEOUT', command: 'fetchOrders' });
  expect(error).not.toHaveProperty('outcome');
});

it('applies the configured deadline to the live SDK host while preserving caller IDs', async () => {
  vi.useFakeTimers();
  const requests: Array<{ op: string }> = [];
  const idle = () => '[]';
  const native: WasmEngineLike = {
    capabilities: () => engineCapabilities('bybit'),
    start: idle, wsOpen: idle, wsMessage: idle, wsClose: idle, wsRouteExhausted: idle,
    httpStart: idle, httpChunk: idle, httpEnd: idle, httpFailed: idle, timer: idle,
    command: (json) => { requests.push(JSON.parse(json)); return '[]'; },
    snapshotFrame: () => new Uint8Array(),
    // Public-only readiness needs decoded market and ticker sections as well as STATUS flags.
    takeFrame: () => new Uint8Array(consumerFrame({
      markets: [], tickers: [],
      loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
    })),
  };
  const construct = function () { return native; };
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const exchange = account(true, { publicDataOnly: true, extra: { rustCommandTimeoutMs: 25 } });
  const startup = exchange.start();
  await vi.advanceTimersByTimeAsync(250);
  await startup;
  const failure = exchange.placeOrders([{
    symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Limit, amount: 1, price: 100,
    clientOrderId: 'caller-one',
  }]).catch((error: unknown) => error);
  await vi.advanceTimersByTimeAsync(25);
  await expect(failure).resolves.toMatchObject({
    code: 'COMMAND_TIMEOUT', command: 'placeOrders', outcome: 'unknown', clientOrderId: 'caller-one',
  });
  expect(requests.filter(({ op }) => op === 'placeOrders')).toHaveLength(1);
  expect(exchange.isDisposed).toBe(false);
});

it('releases the listener and timer when dispatch itself throws', async () => {
  vi.useFakeTimers();
  const exchange = account();
  const off = vi.spyOn(exchange.emitter, 'off');
  exchange.emitter.on('callMethod', () => { throw new Error('Worker transport unavailable'); });
  await expect(exchange.callMethodInWorker('fetchOrders')).rejects.toThrow('Worker transport unavailable');
  expect(off).toHaveBeenCalledWith('callMethod:done', expect.any(Function));
  expect(vi.getTimerCount()).toBe(0);
});

it.each([0, -1, NaN, Infinity, 0.5, 2_147_482_648])('rejects an invalid Rust deadline %s', (rustCommandTimeoutMs) => {
  const store = new DefaultStore();
  const subscribe = vi.spyOn(store, 'subscribe');
  expect(() => new RustEngineExchange('bybit', { key: '', secret: '', extra: { rustCommandTimeoutMs } }, store))
    .toThrow('rustCommandTimeoutMs must be a positive integer');
  expect(subscribe).not.toHaveBeenCalled();
});

it('treats a lost catalogue-refresh worker reply as a read timeout', async () => {
  vi.useFakeTimers();
  const exchange = account(true, { publicDataOnly: true, extra: { rustCommandTimeoutMs: 25 } });
  const failure = exchange.callMethodInWorker('refreshMarkets').catch((error: unknown) => error);
  await vi.advanceTimersByTimeAsync(1025);
  const error = await failure;
  expect(error).toMatchObject({ code: 'COMMAND_TIMEOUT', command: 'refreshMarkets' });
  expect(error).not.toHaveProperty('outcome');
});
