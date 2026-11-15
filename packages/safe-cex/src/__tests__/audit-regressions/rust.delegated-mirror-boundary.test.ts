import type { EngineHost, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
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

function fixture(venue: ExchangeName = 'bybit', mode: ExchangeOptions['signingMode'] = 'delegated', waitAt?: 1 | 2, workerMirror = true) {
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
    key: 'synthetic-key', secret: '11'.repeat(32), workerMirror, signingMode: mode,
    ...(venue === 'lighter' ? { key: '', secret: '01' + '00'.repeat(39), apiKeyIndex: '0', applicationId: '9' } : {}),
  }, new DefaultStore());
  exchanges.push(exchange);
  exchange.setWorkerOwner(() => ({ ownerId: 'synthetic-owner', generation: 1 }));
  adoptEngineCapabilities(exchange);
  const source: string[] = [];
  const attachSource = () => exchange.on('callMethod', (event: { requestId: string; method: string }) => {
    source.push(event.method);
    exchange.emitter.emit('callMethod:done', { requestId: event.requestId, success: true,
      result: event.method.startsWith('place') ? ['source-order'] : [] });
  });
  attachSource();
  return { exchange, native, source, constructed, loader, module, releaseLoader, attachSource };
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

const sourceTransitions = (['local', 'cosigned'] as const).flatMap(mode =>
  [false, true].flatMap(raw => [false, true].map(bridgeReady => ({ mode, raw, bridgeReady }))),
);

it.each(sourceTransitions)('refuses $mode source writes after a DELEGATED switch at the bridge wait (raw=$raw, ready=$bridgeReady)', async ({ mode, raw, bridgeReady }) => {
  const f = fixture('grvt', mode);
  await expect(f.exchange.frameFedReady).resolves.toBe(false);
  expect(Reflect.get(f.exchange, 'pageWriteSupported')).toBe(false);
  if (!bridgeReady) f.exchange.off('callMethod');
  const bridgeWait = vi.spyOn(f.exchange as unknown as { untilWorkerBridge(): Promise<() => boolean> }, 'untilWorkerBridge');
  const pending = raw ? f.exchange.callMethodInWorker('placeOrder', order) : f.exchange.placeOrder(order);
  expect(bridgeWait).toHaveBeenCalledOnce();
  expect(f.exchange.emitter.hasListeners('callMethod')).toBe(bridgeReady);
  expect(f.source).toEqual([]);
  f.exchange.options = { ...f.exchange.options, signingMode: 'delegated' };
  if (!bridgeReady) f.attachSource();
  const result = await pending.then(value => ({ value }), error => ({ error }));
  expect(f.source).toEqual([]);
  expect(result).toMatchObject({ error: { kind: 'unsupported', outcome: 'notSent' } });
  expect(f.native).toEqual([]);
  expect(f.constructed).not.toHaveBeenCalled();
});

it.each(['local', 'cosigned'] as const)('preserves an unchanged %s source write across the bridge wait', async (mode) => {
  const f = fixture('grvt', mode);
  await expect(f.exchange.frameFedReady).resolves.toBe(false);
  f.exchange.off('callMethod');
  const bridgeWait = vi.spyOn(f.exchange as unknown as { untilWorkerBridge(): Promise<() => boolean> }, 'untilWorkerBridge');
  const pending = f.exchange.placeOrder(order);
  expect(bridgeWait).toHaveBeenCalledOnce();
  expect(f.source).toEqual([]);
  f.attachSource();
  await expect(pending).resolves.toEqual(['source-order']);
  expect(f.source).toEqual(['placeOrder']);
  expect(f.native).toEqual([]);
});

it.each(['local', 'cosigned'] as const)('preserves account reads after a %s to DELEGATED switch at the bridge wait', async (mode) => {
  const f = fixture('grvt', mode);
  await expect(f.exchange.frameFedReady).resolves.toBe(false);
  f.exchange.off('callMethod');
  const bridgeWait = vi.spyOn(f.exchange as unknown as { untilWorkerBridge(): Promise<() => boolean> }, 'untilWorkerBridge');
  const pending = f.exchange.fetchBalance();
  expect(bridgeWait).toHaveBeenCalledOnce();
  f.exchange.options = { ...f.exchange.options, signingMode: 'delegated' };
  f.attachSource();
  await expect(pending).resolves.toEqual([]);
  expect(f.source).toEqual(['fetchBalance']);
  expect(f.native).toEqual([]);
  expect(f.constructed).not.toHaveBeenCalled();
});

it('retains the initiating owner generation during the source bridge wait', async () => {
  const f = fixture('grvt', 'local');
  await expect(f.exchange.frameFedReady).resolves.toBe(false);
  let generation = 1;
  f.exchange.setWorkerOwner(() => ({ ownerId: 'synthetic-owner', generation }));
  f.exchange.off('callMethod');
  const bridgeWait = vi.spyOn(f.exchange as unknown as { untilWorkerBridge(): Promise<() => boolean> }, 'untilWorkerBridge');
  const pending = f.exchange.placeOrder(order);
  expect(bridgeWait).toHaveBeenCalledOnce();
  generation += 1;
  f.attachSource();
  await expect(pending).rejects.toMatchObject({ code: 'WORKER_STOPPED', outcome: 'notSent' });
  expect(f.source).toEqual([]);
  expect(f.native).toEqual([]);
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

const signingRequest = {
  id: 'synthetic-signature', exchangeName: 'bybit' as const, type: 'http' as const,
  config: { method: 'POST', url: '/v5/order/create', baseURL: 'https://api.bybit.com', data: {} },
  wsAuth: { toSign: 'synthetic-preimage', outputFormat: 'hex' as const },
};

async function signingFixture(holdKind?: 'credential' | 'http' | 'wsAuth') {
  const f = fixture('bybit', 'local');
  await f.exchange.frameFedReady;
  const host = Reflect.get(f.exchange, 'host') as Pick<EngineHost, 'hostedSigning' | 'hostedSigningClock'>;
  vi.spyOn(host, 'hostedSigningClock').mockReturnValue({ stampMs: 1_700_000_000_000, revision: '1' });
  let release!: () => void;
  const held = new Promise<void>((resolve) => { release = resolve; });
  const signing = vi.spyOn(host, 'hostedSigning').mockImplementation(async (input) => {
    if (input.kind === holdKind) await held;
    return { credential: 'synthetic-key', signature: '00' };
  });
  return { ...f, signing, release };
}

it.each(['signHttpRequest', 'signWsAuth'] as const)('preserves unchanged LOCAL %s through the real signing adapter', async (method) => {
  const f = await signingFixture();
  const result = await f.exchange[method](signingRequest);
  expect(result).not.toHaveProperty('error');
  expect(f.signing.mock.calls.map(([input]) => input.kind)).toEqual(
    method === 'signHttpRequest' ? ['credential', 'http'] : ['wsAuth'],
  );
});

it('blocks the later HTTP signing handoff after a DELEGATED switch during credential lookup', async () => {
  const f = await signingFixture('credential');
  const pending = f.exchange.signHttpRequest(signingRequest);
  await vi.waitFor(() => expect(f.signing).toHaveBeenCalledWith({ kind: 'credential' }));
  f.exchange.options = { ...f.exchange.options, signingMode: 'delegated' };
  f.release();
  const result = await pending;
  expect(f.signing.mock.calls.map(([input]) => input.kind)).toEqual(['credential']);
  expect(result).toHaveProperty('error');
  expect(result.config).toBeUndefined();
});

it.each(['http', 'wsAuth'] as const)('does not publish a retired %s signing result', async (kind) => {
  const f = await signingFixture(kind);
  const pending = kind === 'http' ? f.exchange.signHttpRequest(signingRequest) : f.exchange.signWsAuth(signingRequest);
  await vi.waitFor(() => expect(f.signing.mock.calls.some(([input]) => input.kind === kind)).toBe(true));
  f.exchange.options = { ...f.exchange.options, signingMode: 'delegated' };
  f.release();
  const result = await pending;
  expect(result).toHaveProperty('error');
  expect(result.config).toBeUndefined();
  expect(result.wsAuthSignature).toBeUndefined();
});

it('retains native engine ownership for a DELEGATED source instance', async () => {
  const f = fixture('bybit', 'delegated', undefined, false);
  f.exchange.off('callMethod');
  await f.exchange.loadCapabilities();
  expect(f.constructed).toHaveBeenCalledOnce();
  expect(f.loader).toHaveBeenCalledOnce();
});
