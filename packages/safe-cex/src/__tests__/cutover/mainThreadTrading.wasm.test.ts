// @vitest-environment node
import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { AppSettingsWorkerType } from '../../webworker/AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../../webworker/ExchangeMetadataWorkerType';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { replacementClientOrderIdOf } from '../../exchanges/clientOrderId';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { virtualClock } from '../../utils/virtualClock';
import { ExchangeWorker } from '../../webworker/ExchangeWorker';
import { setMainAcceptsFrames } from '../../webworker/frameForwarding';

// Clock synchronization is host policy; keep the worker lifecycle without a live time read.
vi.mock('../../utils/virtualClock', async (original) => {
  const actual = await original<typeof import('../../utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: async () => {} } };
});

const RESPONSES: Record<string, string> = {
  exchangeInfo: JSON.stringify({
    symbols: [
      {
        symbol: 'BTCUSDT',
        status: 'TRADING',
        baseAsset: 'BTC',
        quoteAsset: 'USDT',
        marginAsset: 'USDT',
        contractType: 'PERPETUAL',
        filters: [
          { filterType: 'PRICE_FILTER', tickSize: '0.10' },
          { filterType: 'LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '1000' },
          { filterType: 'MARKET_LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '120' },
        ],
      },
    ],
  }),
  '24hr': '[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"1","volume":"2","quoteVolume":"3"}]',
  bookTicker: '[{"symbol":"BTCUSDT","bidPrice":"99.9","askPrice":"100.1"}]',
  premiumIndex:
    '[{"symbol":"BTCUSDT","markPrice":"100","indexPrice":"100","lastFundingRate":"0.0001","nextFundingTime":1700000000000}]',
  leverageBracket: '[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125}]}]',
  listenKey: '{"listenKey":"key"}',
  dual: '{"dualSidePosition":false}',
  positionRisk: JSON.stringify([
    {
      symbol: 'BTCUSDT',
      positionSide: 'BOTH',
      positionAmt: '0.5',
      entryPrice: '95',
      unRealizedProfit: '2.5',
      liquidationPrice: '0',
      notional: '50',
      markPrice: '100',
      marginType: 'cross',
      leverage: '20',
      updateTime: 1700000000000,
    },
  ]),
  symbolConfig: '[{"symbol":"BTCUSDT","marginType":"CROSSED","leverage":20}]',
  account:
    '{"assets":[{"asset":"USDT","walletBalance":"1000","availableBalance":"900","initialMargin":"100","unrealizedProfit":"0"}]}',
  openOrders: JSON.stringify([
    {
      orderId: 777,
      symbol: 'BTCUSDT',
      type: 'LIMIT',
      side: 'BUY',
      price: '90',
      origQty: '0.01',
      executedQty: '0',
      reduceOnly: false,
      timeInForce: 'GTC',
      clientOrderId: 'resting-777',
      updateTime: 1700000000000,
    },
  ]),
  openAlgoOrders: '[]',
  order: '{"orderId":4242,"status":"NEW","clientOrderId":"x"}',
  allOpenOrders: '{"code":200,"msg":"The operation of cancel all open order is done."}',
};

/** Canned Binance USD-M venue; records every request the worker's engine sends. */
const venue = () => {
  const requests: string[] = [];
  const transport: Transport = {
    now: () => Date.now(),
    connect: (_url, { onOpen }) => {
      setTimeout(onOpen, 0);
      return { send: () => {}, close: () => {} };
    },
    request: async ({ method, url }) => {
      requests.push(`${method} ${url}`);
      const body = RESPONSES[new URL(url).pathname.split('/').pop()!];
      return { status: body ? 200 : 404, headers: '', body: new TextEncoder().encode(body ?? '{}') };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return { transport, requests, writes: () => requests.filter((request) => !request.startsWith('GET')) };
};

/** Connects a main-thread mirror to a real worker the way the web account controller does. */
const bridge = (worker: () => ExchangeWorker | undefined, main: RustEngineExchange) => ({
  postMessage(message: { op: string; data: unknown }, transfer?: ArrayBuffer[]) {
    const copy = structuredClone(message, transfer ? { transfer } : undefined);
    if (copy.op === 'frame') main.applyWorkerFrame(copy.data as ArrayBuffer);
    // Hosted sessions relay JSON, the strictest bridge: array properties do not survive it.
    if (copy.op === 'callMethod:done') main.emitter.emit(copy.op, JSON.parse(JSON.stringify(copy.data)));
    if (copy.op === 'error') main.emitter.emit('error', ...(Array.isArray(copy.data) ? copy.data : [copy.data]));
  },
  attach() {
    main.on('callMethod', (event: unknown) =>
      worker()!.handlePayload({ op: 'callMethod', data: structuredClone(event) }),
    );
  },
});

describe('main-thread Rust exchange trades through the worker engine', () => {
  let worker: ExchangeWorker | undefined;
  let main: RustEngineExchange | undefined;
  afterEach(() => {
    worker?.dispose();
    main?.dispose();
    worker = main = undefined;
    virtualClock.stop();
    setMainAcceptsFrames(false);
    vi.restoreAllMocks();
  });

  // The web posts full `store` payloads today; `acceptsFrames` is the opt-in frame mode.
  const start = async (frames: boolean) => {
    const tape = venue();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    setMainAcceptsFrames(frames);
    const options = { key: 'k', secret: 's' };
    main = new RustEngineExchange('binance', options, new DefaultStore());
    const ctx = bridge(() => worker, main);
    ctx.attach();
    worker = new ExchangeWorker(
      ctx as unknown as Worker,
      'main-thread-trading',
      'binance',
      { ...options, engine: 'rust' },
      {} as AppSettingsWorkerType,
      {} as ExchangeMetadataWorkerType,
      'main-thread-session',
    );
    expect(await worker.start()).toBe(true);
    expect(worker.exchange).toBeInstanceOf(RustEngineExchange);
    expect(worker.exchange).not.toBe(main);
    const view = frames ? main.store : worker.store;
    await vi.waitFor(() => {
      expect(view.orders.map((order) => order.id)).toEqual(['777']);
      expect(view.positions.find((position) => position.contracts > 0)?.contracts).toBe(0.5);
    });
    return { tape, view };
  };

  it.each([
    ['frame', true],
    ['store payload', false],
  ])(
    'places, amends, cancels and nukes from the main thread on the worker engine (%s bridge)',
    async (_mode, frames) => {
      const { tape, view: store } = await start(frames);

      const ids = await main!.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.0123,
        price: 90000.04,
      });
      expect(ids).toEqual(['4242']);
      expect(tape.writes()).toHaveLength(1);
      expect(tape.writes()[0]).toMatch(/^POST .*\/fapi\/v1\/order\?.*quantity=0\.012.*signature=/);

      const placed = await main!.placeOrderWithClientOrderId({
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.01,
        price: 91,
        clientOrderId: 'main-thread-01',
      });
      expect(placed).toEqual({ orderIds: ['4242'], clientOrderId: 'main-thread-01' });
      expect(tape.writes()[1]).toContain('newClientOrderId=main-thread-01');

      const resting = store.orders.find((order) => order.id === '777')!;
      const amended = await main!.updateOrder({ order: resting, update: { price: 91.5 } });
      expect([...amended]).toEqual(expect.arrayContaining([expect.any(String)]));
      const amendWrites = tape.writes().slice(2);
      expect(amendWrites.length).toBeGreaterThan(0);
      expect(amendWrites.join('\n')).toContain('91.5');
      if (amendWrites.some((write) => write.startsWith('POST')))
        expect(replacementClientOrderIdOf(amended)).toBeTruthy();

      const beforeCancel = tape.writes().length;
      await main!.cancelOrders([resting]);
      const cancel = tape.writes().slice(beforeCancel);
      expect(cancel).toHaveLength(1);
      expect(cancel[0]).toMatch(/^DELETE .*\/fapi\/v1\/(order|batchOrders)\?.*777/);

      const beforeNuke = tape.writes().length;
      await main!.nuke();
      const nuke = tape.writes().slice(beforeNuke);
      const close = nuke.find((write) => write.includes('type=MARKET'))!;
      expect(close).toMatch(/side=SELL/);
      expect(close).toMatch(/quantity=0\.5/);
      expect(close).toMatch(/reduceOnly=true/);
      expect(nuke.some((write) => write.startsWith('DELETE') && write.includes('BTCUSDT'))).toBe(true);
    },
  );

  it('reports a worker-side preflight rejection once and sends nothing', async () => {
    const { tape } = await start(true);
    const errors: unknown[] = [];
    main!.on('error', (error: unknown) => errors.push(error));
    const failure = await main!
      .placeOrderWithClientOrderId({
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.01,
        price: 91,
        clientOrderId: 'not valid!',
      })
      .catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(Error);
    expect((failure as Error).message).toContain('clientOrderId must');
    expect(errors).toHaveLength(1);
    expect(tape.writes()).toEqual([]);
  });

  it('times out an unanswered placement as an unknown outcome carrying the caller ID', async () => {
    main = new RustEngineExchange(
      'binance',
      { key: 'k', secret: 's', extra: { rustCommandTimeoutMs: 1 } },
      new DefaultStore(),
    );
    main.on('callMethod', () => {});
    const failure = await main
      .placeOrderWithClientOrderId({
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.01,
        price: 91,
        clientOrderId: 'lost-01',
      })
      .catch((error: unknown) => error);
    expect(failure).toMatchObject({
      code: 'COMMAND_TIMEOUT',
      command: 'placeOrderWithClientOrderId',
      outcome: 'unknown',
      clientOrderIds: ['lost-01'],
    });
  });

  it('keeps an unstarted instance without a worker bridge local, sending nothing', async () => {
    main = new RustEngineExchange('binance', { key: 'k', secret: 's' }, new DefaultStore());
    const dispatch = vi.spyOn(main.emitter, 'dispatch');
    await expect(main.cancelSymbolOrders('BTCUSDT')).rejects.toThrow('engine is not started');
    expect(dispatch).not.toHaveBeenCalled();
  });
});
