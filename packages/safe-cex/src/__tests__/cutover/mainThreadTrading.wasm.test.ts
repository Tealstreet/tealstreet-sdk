// @vitest-environment node
import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { AppSettingsWorkerType } from '../../webworker/AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../../webworker/ExchangeMetadataWorkerType';

import { webcrypto } from 'node:crypto';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';
import { OrderSide, OrderType } from '../../types';
import { virtualClock } from '../../utils/virtualClock';
import { ExchangeWorker } from '../../webworker/ExchangeWorker';
import { setMainAcceptsFrames } from '../../webworker/frameForwarding';

// Clock synchronization is host policy; keep the worker lifecycle without a live time read.
vi.mock('../../utils/virtualClock', async (original) => {
  const actual = await original<typeof import('../../utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: async () => {} } };
});

const positionRisk = (positionAmt: string) =>
  JSON.stringify([
    {
      symbol: 'BTCUSDT',
      positionSide: 'BOTH',
      positionAmt,
      entryPrice: '95',
      unRealizedProfit: '2.5',
      liquidationPrice: '0',
      notional: '50',
      markPrice: '100',
      marginType: 'cross',
      leverage: '20',
      updateTime: 1700000000000,
    },
  ]);

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
  positionRisk: positionRisk('0.5'),
  symbolConfig: '[{"symbol":"BTCUSDT","marginType":"CROSSED","leverage":20}]',
  account:
    '{"assets":[{"asset":"USDT","walletBalance":"1000","availableBalance":"900","initialMargin":"100","unrealizedProfit":"0"}]}',
  openOrders: JSON.stringify(
    [777, 778].map((orderId) => ({
      orderId,
      symbol: 'BTCUSDT',
      type: 'LIMIT',
      side: 'BUY',
      price: '90',
      origQty: '0.01',
      executedQty: '0',
      reduceOnly: false,
      timeInForce: 'GTC',
      clientOrderId: `resting-${orderId}`,
      updateTime: 1700000000000,
    })),
  ),
  openAlgoOrders: '[]',
  order: '{"orderId":4242,"status":"NEW","clientOrderId":"x"}',
  allOpenOrders: '{"code":200,"msg":"The operation of cancel all open order is done."}',
  algoOpenOrders: '{"code":200,"msg":"success"}',
  trades: '[{"id":1,"price":"100.5","qty":"0.2","quoteQty":"20.1","time":1700000000000,"isBuyerMaker":true}]',
};

/** Canned Binance USD-M venue; records every request the worker's engine sends. */
const venue = (positionRead?: (closes: number) => { status: number; body: string }) => {
  const requests: string[] = [];
  let closes = 0;
  // A market close flattens the account: Nuke reads flat back from the stream before it reports.
  const flatten: Array<() => void> = [];
  const transport: Transport = {
    prepareHttp: prepareRecordedHttp,
    now: () => Date.now(),
    connect: (_url, { onOpen, onMessage }) => {
      setTimeout(onOpen, 0);
      flatten.push(() =>
        onMessage(
          '{"e":"ACCOUNT_UPDATE","E":1700000001000,"T":1700000001000,"a":{"m":"ORDER","B":[],"P":[{"s":"BTCUSDT","pa":"0","ep":"0","cr":"0","up":"0","mt":"cross","iw":"0","ps":"BOTH"}]}}',
        ),
      );
      return { send: () => {}, close: () => {} };
    },
    request: async ({ method, url }) => {
      requests.push(`${method} ${url}`);
      const parsed = new URL(url);
      const name = parsed.pathname.split('/').pop()!;
      if (method === 'POST' && parsed.searchParams.get('type') === 'MARKET') {
        closes += 1;
        setTimeout(() => flatten.forEach((send) => send()), 0);
      }
      if (name === 'positionRisk' && closes > 0) {
        const { status, body } = positionRead?.(closes) ?? { status: 200, body: positionRisk('0') };
        return { status, headers: '', body: new TextEncoder().encode(body) };
      }
      // A single cancel is acknowledged with the cancelled order, as Binance USD-M does.
      const body =
        method === 'DELETE' && name === 'order'
          ? JSON.stringify({
              orderId: Number(parsed.searchParams.get('orderId')),
              symbol: parsed.searchParams.get('symbol'),
              status: 'CANCELED',
              origQty: '0.01',
              executedQty: '0',
            })
          : RESPONSES[name];
      return { status: body ? 200 : 404, headers: '', body: new TextEncoder().encode(body ?? '{}') };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  // Trading writes only: the private socket's listen-key session is not an order.
  const writes = () => requests.filter((request) => !request.startsWith('GET') && !request.includes('/listenKey'));
  return { transport, requests, writes };
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

const previousWindowCrypto = Object.getOwnPropertyDescriptor(window, 'crypto');

describe('main-thread Rust exchange trades through the worker engine', () => {
  let worker: ExchangeWorker | undefined;
  let main: RustEngineExchange | undefined;
  afterEach(() => {
    worker?.dispose();
    main?.dispose();
    worker = main = undefined;
    virtualClock.stop();
    setMainAcceptsFrames(false);
    if (previousWindowCrypto) Object.defineProperty(window, 'crypto', previousWindowCrypto);
    else delete (window as { crypto?: Crypto }).crypto;
    vi.restoreAllMocks();
  });

  // The web posts full `store` payloads today; `acceptsFrames` is the opt-in frame mode.
  const start = async (frames: boolean, positionRead?: Parameters<typeof venue>[0]) => {
    // SDK setup supplies a minimal window in Node; the engine generates Binance
    // client order IDs from the first crypto source it finds there.
    Object.defineProperty(window, 'crypto', { configurable: true, value: webcrypto });
    const tape = venue(positionRead);
    const loader = vi.fn(loadNodeEngine);
    setWasmEngineLoader(loader);
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
      expect(view.orders.map((order) => order.id).sort()).toEqual(['777', '778']);
      expect(view.positions.find((position) => position.contracts > 0)?.contracts).toBe(0.5);
    });
    return { tape, view, loader };
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

      // Binance USD-M has no amend: the engine cancels, then places the replacement.
      const amended = store.orders.find((order) => order.id === '777')!;
      const beforeAmend = tape.writes().length;
      expect([...(await main!.updateOrder({ order: amended, update: { price: 91.5 } }))]).toEqual(['4242']);
      const amend = tape.writes().slice(beforeAmend);
      expect(amend[0]).toMatch(/^DELETE .*\/fapi\/v1\/order\?.*orderId=777/);
      expect(amend.at(-1)).toMatch(/^POST .*\/fapi\/v1\/order\?.*price=91\.5/);

      const cancelled = store.orders.find((order) => order.id === '778')!;
      const beforeCancel = tape.writes().length;
      await main!.cancelOrders([cancelled]);
      const cancel = tape.writes().slice(beforeCancel);
      expect(cancel).toHaveLength(1);
      expect(cancel[0]).toMatch(/^DELETE .*\/fapi\/v1\/order\?.*orderId=778/);

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

  const closesIn = (writes: string[]) => writes.filter((write) => write.includes('type=MARKET'));

  it.each([
    ['frame', true],
    ['store payload', false],
  ])('Nuke sends a second close sized from a read after the first (%s bridge)', async (_mode, frames) => {
    // The stream reports flat after the first close; only the venue read knows better.
    const { tape } = await start(frames, (closes) => ({
      status: 200,
      body: positionRisk(closes === 1 ? '0.2' : '0'),
    }));
    await main!.nuke();
    const closes = closesIn(tape.writes());
    expect(closes).toHaveLength(2);
    expect(closes[0]).toMatch(/quantity=0\.5/);
    expect(closes[1]).toMatch(/quantity=0\.2/);
    expect(closes[1]).toMatch(/reduceOnly=true/);
    const secondClose = tape.requests.indexOf(closes[1]);
    const firstClose = tape.requests.indexOf(closes[0]);
    expect(tape.requests.slice(firstClose, secondClose).some((request) => request.includes('/positionRisk'))).toBe(
      true,
    );
  });

  it.each([
    ['frame', true],
    ['store payload', false],
  ])('Nuke sends no second close when the read fails, and says not confirmed (%s bridge)', async (_mode, frames) => {
    const { tape } = await start(frames, () => ({ status: 500, body: '{"code":-1000,"msg":"down"}' }));
    await expect(main!.nuke()).rejects.toThrow(/could not confirm the account is flat/);
    expect(closesIn(tape.writes())).toHaveLength(1);
  });

  it('reads public trades and adopts capabilities from the worker without a main-thread engine', async () => {
    const { tape, loader } = await start(true);
    await expect(main!.fetchPublicTrades('BTCUSDT')).resolves.toEqual([
      expect.objectContaining({ id: '1', symbol: 'BTCUSDT', price: 100.5, side: 'sell' }),
    ]);
    expect(tape.requests.filter((request) => request.includes('/fapi/v1/trades?'))).toHaveLength(1);
    await vi.waitFor(() =>
      expect(main!.engineCapabilities).toEqual((worker!.exchange as RustEngineExchange).engineCapabilities),
    );
    await expect(main!.loadCapabilities()).resolves.toEqual(worker!.exchange!.capabilities);
    expect(main!.hasStarted).toBe(false);
    // Only the worker built an engine.
    expect(loader).toHaveBeenCalledTimes(1);
  });

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
