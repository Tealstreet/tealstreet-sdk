import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions, Order } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';
import { consumerFrame } from '../cutover/consumerFrame';
import { adoptEngineCapabilities, engineCapabilities } from '../fixtures/engineCapabilities';

const order: Order = {
  id: 'synthetic-order', symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy,
  price: 100, amount: 1, filled: 0, remaining: 1, reduceOnly: false,
  status: OrderStatus.Open, lastUpdate: 1,
};
const writes: Record<string, unknown[]> = {
  placeOrder: [order], placeOrders: [[order]], placeOrderWithClientOrderId: [order],
  updateOrder: [{ order, update: { price: 101 } }], cancelOrders: [[order]],
  cancelSymbolOrders: ['BTCUSDT'], cancelAllOrders: [['BTCUSDT']], nuke: [], nukePositions: [], nukeOrders: [],
  setLeverage: ['BTCUSDT', 5], setAllLeverage: [5], changeMarginType: [false, 'BTCUSDT'],
  changePositionMode: [false, 'BTCUSDT'], changeStpMode: ['EXPIRE_TAKER'],
  changeAccountTier: ['premium'], scheduleCancel: [undefined],
};
const exchanges: RustEngineExchange[] = [];
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

function fixture(venue: ExchangeName = 'bybit', mode: ExchangeOptions['signingMode'] = 'delegated', waitAt?: 1 | 2) {
  const native: string[] = [];
  const idle = () => '[]';
  const frame = () => new Uint8Array(consumerFrame({ markets: [], tickers: [], positions: [], orders: [],
    loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } }));
  const engine: WasmEngineLike = {
    syncHostTime: () => true, capabilities: () => engineCapabilities(venue),
    construction: () => JSON.stringify({ frameFed: true, canSign: true, publicDataOnly: false }),
    start: idle, wsOpen: idle, wsMessage: idle, wsClose: idle, wsRouteExhausted: idle,
    httpStart: idle, httpChunk: idle, httpEnd: idle, httpFailed: idle, timer: idle,
    httpCommandOwner: () => undefined, applyFrame: idle, snapshotFrame: frame, takeFrame: frame,
    command: (json) => {
      const command = JSON.parse(json);
      native.push(command.op);
      const result = command.op === 'fetchPositionsFresh' ? { skipped: [], open: [] }
        : ['placeOrders', 'amendOrder'].includes(command.op) ? ['native-order'] : null;
      return command.command === undefined ? '[]' : JSON.stringify([
        { op: 'done', command: command.command, result: JSON.stringify(result) },
      ]);
    },
  };
  const constructed = vi.fn(function () { return engine; });
  const module = { WasmEngine: Object.assign(constructed, {
    frameFedSupported: () => ['bybit', 'lighter', 'hyperliquid'].includes(venue),
  }) as unknown as WasmEngineModule['WasmEngine'] };
  let releaseLoader!: () => void;
  const held = new Promise<void>((resolve) => { releaseLoader = resolve; });
  const loader = vi.fn(async () => {
    if (loader.mock.calls.length === waitAt) await held;
    return module;
  });
  setWasmEngineLoader(loader);
  const exchange = new RustEngineExchange(venue, {
    key: 'synthetic-key', secret: '11'.repeat(32), workerMirror: true, signingMode: mode,
    ...(venue === 'lighter' ? { key: '', secret: '01' + '00'.repeat(39), apiKeyIndex: '0', applicationId: '9' } : {}),
  }, new DefaultStore());
  exchanges.push(exchange);
  exchange.setWorkerOwner(() => ({ ownerId: 'synthetic-owner', generation: 1 }));
  adoptEngineCapabilities(exchange);
  const source: string[] = [];
  exchange.on('callMethod', (event: { requestId: string; method: string }) => {
    source.push(event.method);
    exchange.emitter.emit('callMethod:done', { requestId: event.requestId, success: true,
      result: event.method.startsWith('place') ? ['source-order'] : [] });
  });
  return { exchange, native, source, constructed, loader, module, releaseLoader };
}

it.each(Object.keys(writes))('DELEGATED %s uses neither the page engine nor generic source writer', async (method) => {
  const venue = method === 'changeAccountTier' ? 'lighter' : method === 'scheduleCancel' ? 'hyperliquid'
    : method === 'changeStpMode' ? 'asterdex' : 'bybit';
  for (const raw of [false, true]) {
    const f = fixture(venue);
    await f.exchange.frameFedReady;
    const members = f.exchange as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>;
    const rawCall = f.exchange.callMethodInWorker as (method: string, ...args: unknown[]) => Promise<unknown>;
    const pending = raw ? rawCall(method, ...writes[method])
      : members[method](...writes[method]);
    await expect(pending).rejects.toMatchObject({ outcome: 'notSent' });
    expect(f.native).toEqual([]);
    expect(f.source).toEqual([]);
    expect(f.constructed).not.toHaveBeenCalled();
    expect(f.loader).not.toHaveBeenCalled();
  }
});

it.each(['local', 'cosigned'] as const)('preserves %s page and structurally unsupported source writers', async (mode) => {
  for (const venue of ['bybit', 'grvt'] as const) {
    const f = fixture(venue, mode);
    await f.exchange.frameFedReady;
    await expect(f.exchange.placeOrder(order)).resolves.toEqual([venue === 'bybit' ? 'native-order' : 'source-order']);
    expect(f.native).toEqual(venue === 'bybit' ? ['placeOrders'] : []);
    expect(f.source).toEqual(venue === 'grvt' ? ['placeOrder'] : []);
  }
});

it('DELEGATED public candles and account reads stay on the source without loading WASM', async () => {
  const f = fixture();
  await f.exchange.frameFedReady;
  await f.exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '1m', from: 1, to: 2 });
  await f.exchange.fetchBalance();
  expect(f.source).toEqual(['fetchOHLCV', 'fetchBalance']);
  expect(f.loader).not.toHaveBeenCalled();
  expect(f.constructed).not.toHaveBeenCalled();
});

it.each(['signHttpRequest', 'signWsAuth'] as const)('DELEGATED refuses local %s without loading WASM', async (method) => {
  const f = fixture();
  await f.exchange.frameFedReady;
  const result = await f.exchange[method]({ id: 'synthetic-signature', payload: '{}' } as never);
  expect(result).toHaveProperty('error');
  expect(f.loader).not.toHaveBeenCalled();
  expect(f.constructed).not.toHaveBeenCalled();
});

it.each(['placeOrder', 'changeAccountTier', 'scheduleCancel'])('a kept mirror refuses %s after changing to DELEGATED', async (method) => {
  const f = fixture(method === 'changeAccountTier' ? 'lighter' : method === 'scheduleCancel' ? 'hyperliquid' : 'bybit', 'local');
  await expect(f.exchange.frameFedReady).resolves.toBe(true);
  f.exchange.options = { ...f.exchange.options, signingMode: 'delegated' };
  const members = f.exchange as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>;
  await expect(members[method](...writes[method])).rejects.toMatchObject({ outcome: 'notSent' });
  expect(f.native).toEqual([]);
  expect(f.source).toEqual([]);
});

it.each([1, 2] as const)('does not construct an engine after a DELEGATED switch at loader wait %s', async (waitAt) => {
  const f = fixture('bybit', 'local', waitAt);
  await vi.waitFor(() => expect(f.loader).toHaveBeenCalledTimes(waitAt));
  f.exchange.options = { ...f.exchange.options, signingMode: 'delegated' };
  f.releaseLoader();
  await expect(f.exchange.frameFedReady).resolves.toBe(false);
  expect(f.constructed).not.toHaveBeenCalled();
  expect(f.native).toEqual([]);
  expect(f.source).toEqual([]);
});

it.each(['signHttpRequest', 'signWsAuth'] as const)('retires %s across a ready-host continuation', async (method) => {
  const f = fixture('bybit', 'local');
  await f.exchange.frameFedReady;
  const host = Reflect.get(f.exchange, 'host') as { hostedSigning(input: unknown): Promise<{ credential: string; signature: string }> };
  const signing = vi.spyOn(host, 'hostedSigning').mockResolvedValue({ credential: 'synthetic-key', signature: '00' });
  const request = { id: 'synthetic-signature', exchangeName: 'bybit' as const, type: 'http' as const,
    config: { method: 'POST', url: '/v5/order/create', baseURL: 'https://api.bybit.com', data: {} },
    wsAuth: { toSign: 'synthetic-preimage', outputFormat: 'hex' as const } };
  const pending = f.exchange[method](request);
  f.exchange.options = { ...f.exchange.options, signingMode: 'delegated' };
  expect(await pending).toHaveProperty('error');
  expect(signing).not.toHaveBeenCalled();
});
