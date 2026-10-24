import { installNativeStartupFrames } from './rust.native-startup.fixtures';
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions, OHLCVOptions, Order } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { isRustVenue, RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';

const USER = '0x1111111111111111111111111111111111111111';
const SIGNER = '0x2222222222222222222222222222222222222222';
const PRIVATE = `0x${'12'.repeat(32)}`;
const running: RustEngineExchange[] = [];

const fixture = (
  options: Partial<ExchangeOptions> = {},
  name: ExchangeName = 'asterdex',
  nativeVersion?: 'v1' | 'v3',
) => {
  const commands: Array<Record<string, unknown>> = [];
  const nativeSettings = { isHedged: false };
  const noActions = () => '[]';
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities(name),
    ...(nativeVersion ? { construction: () => JSON.stringify({ asterdexApiVersion: nativeVersion }) } : {}),
    start: noActions,
    command(json) {
      const command = JSON.parse(json);
      if (command.op === 'setTickerSymbols' || command.op === 'setPollScale') return '[]';
      commands.push(command);
      return JSON.stringify([
        {
          op: 'done',
          command: command.command,
          result: JSON.stringify(
            command.op === 'fetchStpMode'
              ? 'EXPIRE_MAKER'
              : command.op === 'fetchAccount'
                ? { userId: 'documented-fixture' }
                : command.op === 'fetchLeverage'
                  ? { buyLeverage: 10, sellLeverage: 10 }
                  : command.op === 'placeOrders'
                    ? ['newly-acked']
                    : [],
          ),
        },
      ]);
    },
    takeFrame: () => new Uint8Array(consumerFrame({
      loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
      options: nativeSettings,
    })),
    snapshotFrame: () => new Uint8Array(),
    wsOpen: noActions,
    wsMessage: noActions,
    wsClose: noActions,
    httpStart: noActions,
    httpChunk: noActions,
    httpEnd: noActions,
    httpFailed: noActions,
    timer: noActions,
  };
  const construct = vi.fn(function (_venue: string, _options: string, _setup?: string) {
    return engine;
  });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  installNativeStartupFrames(engine, store, () => ({ options: nativeSettings }));
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  const exchange = new RustEngineExchange(
    name,
    { key: SIGNER, secret: PRIVATE, applicationId: USER, ...options },
    store,
  );
  running.push(exchange);
  const setNativeHedged = (isHedged: boolean) => { nativeSettings.isHedged = isHedged; };
  return { exchange, construct, commands, store, setNativeHedged, engine };
};

describe('Aster Rust wrapper account and typed command boundary', () => {
  afterEach(() => {
    for (const exchange of running.splice(0)) exchange.dispose();
    vi.restoreAllMocks();
  });

  it('registers Aster and sends wallet and read-agent material only through signer setup', async () => {
    const { exchange, construct } = fixture({
      secondary_key: USER,
      secondary_secret: PRIVATE,
      extra: { asterBuilderAddress: SIGNER, asterBuilderFeeRateMaker: '0.00002', asterBuilderFeeRateTaker: '0.00005' },
    });
    expect(isRustVenue('asterdex')).toBe(true);
    await exchange.start();
    expect(construct.mock.calls[0][0]).toBe('asterdex');
    const options = JSON.parse(construct.mock.calls[0][1]);
    expect(options).toMatchObject({
      applicationId: USER,
      publicDataOnly: false,
      asterdex: {
        apiVersion: 'v3',
        signerAddress: SIGNER,
        readSignerAddress: USER,
        builderAddress: SIGNER,
        builderFeeRateMaker: '0.00002',
        builderFeeRateTaker: '0.00005',
      },
    });
    expect(options).not.toHaveProperty('key');
    expect(options).not.toHaveProperty('secret');
    expect(options.asterdex).not.toHaveProperty('readKey');
    expect(options.asterdex).not.toHaveProperty('readSecret');
    expect(JSON.parse(construct.mock.calls[0][2] ?? 'null')).toEqual({
      primary: { key: SIGNER, secret: PRIVATE, secretEncoding: 'hex' },
      secondary: { key: USER, secret: PRIVATE, secretEncoding: 'hex' },
    });
  });

  it('preserves explicit API mode and auth precedence without changing flat account fields', async () => {
    const { exchange, construct } = fixture({
      key: 'legacy-key',
      secret: 'legacy-secret',
      auth: { userAddress: USER, signerAddress: SIGNER, privateKey: PRIVATE },
      extra: { asterdexApiVersion: 'v3' },
    });
    await exchange.start();
    const options = JSON.parse(construct.mock.calls[0][1]);
    expect(options).toMatchObject({ asterdex: { apiVersion: 'v3', userAddress: USER, signerAddress: SIGNER } });
    expect(options.asterdex).not.toHaveProperty('privateKey');
    // The auth wallet key wins over the flat secret, as before.
    expect(JSON.parse(construct.mock.calls[0][2] ?? 'null')).toEqual({
      primary: { key: 'legacy-key', secret: PRIVATE, secretEncoding: 'hex' },
    });
  });

  it('sends the setup-resolved version and adopts native resolution for canSign and STP', async () => {
    const { exchange, construct } = fixture({ secret: '', extra: { asterdexApiVersion: 'v1' } }, 'asterdex', 'v3');
    // Before async WASM loading, preserve the synchronous v1/no-secret answer.
    expect(exchange.isPublicDataOnly).toBe(true);
    await exchange.start();
    expect(JSON.parse(construct.mock.calls[0][1]).asterdex.apiVersion).toBe('v1');
    // Native v3 requires a full auth context; an explicit remote signer still wins.
    expect(exchange.isPublicDataOnly).toBe(true);
    await expect(exchange.fetchStpMode()).resolves.toBe('EXPIRE_MAKER');

    const inferred = fixture();
    await inferred.exchange.start();
    // Setup resolves the version from the material's shape; the venue never guesses.
    expect(JSON.parse(inferred.construct.mock.calls[0][1]).asterdex.apiVersion).toBe('v3');
  });

  it('uses native version for signing eligibility after host creation', async () => {
    const { exchange } = fixture({ key: 'legacy', secret: 'legacy', applicationId: undefined }, 'asterdex', 'v3');
    expect(exchange.isPublicDataOnly).toBe(false);
    await exchange.start();
    expect(exchange.isPublicDataOnly).toBe(true);
  });

  it('keeps valid auth-only V3 accounts public before and after host creation, as TS does', async () => {
    const { exchange } = fixture({
      key: '',
      secret: '',
      applicationId: undefined,
      auth: { userAddress: USER, signerAddress: SIGNER, privateKey: PRIVATE },
    }, 'asterdex', 'v3');
    expect(exchange.isPublicDataOnly).toBe(true);
    await exchange.start();
    expect(exchange.isPublicDataOnly).toBe(true);
  });

  it('passes typed per-order and account STP commands through the engine', async () => {
    const { exchange, commands } = fixture();
    await exchange.start();
    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      amount: 1,
      price: 100,
      stpMode: 'EXPIRE_BOTH',
    });
    expect(await exchange.fetchStpMode()).toBe('EXPIRE_MAKER');
    await exchange.changeStpMode('EXPIRE_TAKER');
    expect(commands).toMatchObject([
      { op: 'placeOrders', orders: [{ stpMode: 'EXPIRE_BOTH' }] },
      { op: 'fetchStpMode' },
      { op: 'setStpMode', mode: 'EXPIRE_TAKER' },
    ]);
  });

  it.each(['binance', 'binancepm', 'binancespot', 'binanceinverse', 'bybit', 'kucoin'] as const)(
    'rejects Aster-only STP intent on %s before any single or mixed-batch engine command',
    async (name) => {
      const { exchange, commands } = fixture(name === 'kucoin' ? { passphrase: 'fixture-passphrase' } : {}, name);
      await exchange.start();
      const order = { symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Limit, amount: 1, price: 100 };
      const stpOrder = { ...order, stpMode: 'EXPIRE_BOTH' as const };
      await expect(exchange.placeOrder(stpOrder)).rejects.toThrow(/does not support.*stpMode/i);
      await expect(exchange.placeOrders([order, stpOrder])).rejects.toThrow(/does not support.*stpMode/i);
      expect(commands).toEqual([]);
      await expect(exchange.placeOrder(order)).resolves.toEqual(['newly-acked']);
      expect(commands).toMatchObject([{ op: 'placeOrders', orders: [order] }]);
    },
  );

  it('validates every Aster per-order STP option before a mixed-batch engine command', async () => {
    const order = { symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Limit, amount: 1, price: 100 };
    const v1 = fixture({ key: 'legacy', secret: 'secret', applicationId: undefined, extra: { asterdexApiVersion: 'v1' } });
    await v1.exchange.start();
    await expect(v1.exchange.placeOrders([order, { ...order, stpMode: 'EXPIRE_BOTH' }])).rejects.toThrow(/requires API v3/);
    expect(v1.commands).toEqual([]);

    const v3 = fixture();
    await v3.exchange.start();
    await expect(v3.exchange.placeOrders([order, { ...order, stpMode: 'NONE' as 'EXPIRE_BOTH' }])).rejects.toThrow(/Invalid AsterDEX/);
    expect(v3.commands).toEqual([]);
  });

  it('rejects unsupported caller IDs before submitting an engine command', async () => {
    const { exchange, commands } = fixture();
    await exchange.start();
    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Market,
        amount: 1,
        clientOrderId: 'caller',
      }),
    ).rejects.toThrow(/clientOrderId/i);
    expect(commands).toEqual([]);
  });

  it('rejects STP for V1 and malformed STP modes before commands', async () => {
    const { exchange, commands } = fixture({
      key: 'legacy',
      secret: 'secret',
      applicationId: undefined,
      extra: { asterdexApiVersion: 'v1' },
    });
    await exchange.start();
    await expect(exchange.fetchStpMode()).rejects.toThrow(/requires API v3/);
    await expect(exchange.changeStpMode('EXPIRE_BOTH')).rejects.toThrow(/requires API v3/);
    expect(commands).toEqual([]);
  });

  it('rejects an invalid runtime STP string without an engine command', async () => {
    const { exchange, commands } = fixture();
    await exchange.start();
    await expect(exchange.changeStpMode('NONE' as 'EXPIRE_TAKER')).rejects.toThrow(/Invalid AsterDEX/);
    expect(commands).toEqual([]);
  });

  it('serves engine-owned public metadata and the account alias query', async () => {
    const { exchange, store, commands, setNativeHedged, engine } = fixture();
    store.update({
      markets: [
        {
          id: 'BTCUSDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { price: 0.01, amount: 0.001 },
          limits: { amount: { min: 0.001, max: 2 }, leverage: { min: 1, max: 125 } },
        },
      ],
    });
    setNativeHedged(true);
    await exchange.start();
    expect(await exchange.fetchMarkets()).toBe(store.markets);
    expect(await exchange.fetchTickers()).toBe(store.tickers);
    expect(await exchange.fetchPositionMode()).toBe(true);
    expect(await exchange.fetchSymbolMaxLeverage('BTCUSDT')).toBe(125);
    expect(await exchange.getAccount()).toEqual({ userId: 'documented-fixture' });
    expect(commands.at(-1)).toMatchObject({ op: 'fetchAccount' });
  });

  it('preserves cancellation symbols before Store sees a placement and for external orders', async () => {
    const { exchange, store, commands } = fixture();
    await exchange.start();
    const [id] = await exchange.placeOrder({ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 100 });
    expect(id).toBe('newly-acked');
    expect(store.orders).toEqual([]);
    await exchange.cancelOrders([{ id, symbol: 'BTCUSDT', type: OrderType.Limit } as Order]);
    await exchange.cancelOrders([{ id: 'external', symbol: 'ETHUSDT', type: OrderType.StopLoss } as Order]);
    expect(commands.slice(-2)).toMatchObject([
      { op: 'cancelOrdersWithSymbols', orders: [{ id: 'newly-acked', symbol: 'BTCUSDT', type: OrderType.Limit }] },
      { op: 'cancelOrdersWithSymbols', orders: [{ id: 'external', symbol: 'ETHUSDT', type: OrderType.StopLoss }] },
    ]);
    expect(store.orders).toEqual([]);
  });

  it('loads account leverage 10 rather than the market maximum 125 into fetched leverage', async () => {
    const { exchange, store, commands } = fixture();
    store.update({ markets: [{ id: 'BTCUSDT', symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', active: true,
      precision: { price: 0.01, amount: 0.001 }, limits: { amount: { min: 0.001, max: 2 }, leverage: { min: 1, max: 125 } } }] });
    await exchange.start();
    expect(await exchange.fetchSymbolMaxLeverage('BTCUSDT')).toBe(125);
    expect(await exchange.fetchLeverageForSymbol('BTCUSDT')).toEqual({ buyLeverage: 10, sellLeverage: 10 });
    expect(store.fetchedLeverage.BTCUSDT).toEqual({ buyLeverage: 10, sellLeverage: 10 });
    expect(commands.at(-1)).toMatchObject({ op: 'fetchLeverage', symbol: 'BTCUSDT' });
  });

  it('keeps duplicate registrations of the same book callback independently active', async () => {
    const { exchange, commands } = fixture();
    await exchange.start();
    const listener = vi.fn();
    const first = exchange.listenOrderBook('BTCUSDT', listener);
    const second = exchange.listenOrderBook('BTCUSDT', listener);
    expect(commands.filter((command) => command.op === 'subscribeOrderBook')).toHaveLength(1);
    first();
    first();
    expect(commands.filter((command) => command.op === 'unsubscribeOrderBook')).toHaveLength(0);
    second();
    expect(commands.filter((command) => command.op === 'unsubscribeOrderBook')).toHaveLength(1);
  });

  // The bridge no longer substitutes 1h: the engine rejects what Aster cannot serve.
  it('forwards Aster candle intervals unchanged for reads and the subscription lifecycle', async () => {
    const { exchange, commands } = fixture();
    await exchange.start();
    const request = { symbol: 'BTCUSDT', interval: '1s' as OHLCVOptions['interval'] };
    await exchange.fetchOHLCV(request);
    const unsubscribe = exchange.listenOHLCV(request, () => undefined);
    unsubscribe();
    expect(
      commands
        .filter((command) => ['fetchCandles', 'subscribeCandles', 'unsubscribeCandles'].includes(String(command.op)))
        .map(({ op, interval }) => ({ op, interval })),
    ).toEqual([
      { op: 'fetchCandles', interval: '1s' },
      { op: 'subscribeCandles', interval: '1s' },
      { op: 'unsubscribeCandles', interval: '1s' },
    ]);
  });
});
