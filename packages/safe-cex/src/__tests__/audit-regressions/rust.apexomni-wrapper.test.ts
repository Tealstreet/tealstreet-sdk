import { installNativeStartupFrames } from './rust.native-startup.fixtures';
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeOptions, Market, PlaceOrderOpts, StoreData } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { isRustVenue, RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';

const MARKET: Market = {
  id: 'BTC-USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 10 }, leverage: { min: 1, max: 100 } },
  info: {},
};
const placement = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 1,
  price: 100,
  ...overrides,
});
const fixture = (options: Partial<ExchangeOptions> = {}, initialLoaded: Partial<StoreData['loaded']> = {}) => {
  // Native STATUS, rather than the seeded JS store, controls startup readiness.
  const nativeLoaded = { markets: true, tickers: true, balance: true, orders: true, positions: true, ...initialLoaded };
  const result = vi.fn((_command: Record<string, unknown>): unknown => []);
  const commands: Array<Record<string, unknown>> = [];
  const noActions = () => '[]';
  const start = vi.fn(noActions);
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities('apexomni'),
    start,
    command: (json) => {
      // The caller wait is engine receipt metadata (pinned by the host tests),
      // not part of the native command these regressions compare.
      const { timeoutMs: _wait, ...command } = JSON.parse(json);
      if (command.op === 'setTickerSymbols' || command.op === 'setPollScale') return noActions();
      commands.push(command);
      return JSON.stringify([{ op: 'done', command: command.command, result: JSON.stringify(result(command)) }]);
    },
    takeFrame: () => new Uint8Array(consumerFrame({ loaded: nativeLoaded })),
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
  const construct = vi.fn(function (_venue: string, _options: string) {
    return engine;
  });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  installNativeStartupFrames(engine, store, () => ({ loaded: nativeLoaded }));
  store.update({
    markets: [MARKET],
    loaded: { ...nativeLoaded },
  });
  const exchange = new RustEngineExchange(
    'apexomni',
    {
      key: 'api-key',
      secret: 'api-secret',
      passphrase: 'api-passphrase',
      omniKeySeed: '11'.repeat(65),
      ...options,
    },
    store,
  );
  const setNativeLoaded = (loaded: Partial<StoreData['loaded']>) => Object.assign(nativeLoaded, loaded);
  return { exchange, store, construct, commands, result, start, setNativeLoaded };
};

describe('Rust ApeX Omni SDK boundary', () => {
  const exchanges: RustEngineExchange[] = [];
  const setup = (options: Partial<ExchangeOptions> = {}, loaded: Partial<StoreData['loaded']> = {}) => {
    const f = fixture(options, loaded);
    exchanges.push(f.exchange);
    return f;
  };
  afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

  it('registers the venue and forwards the Omni seed with all API credentials to WASM', async () => {
    expect(isRustVenue('apexomni')).toBe(true);
    const { exchange, construct } = setup({ testnet: true });
    await exchange.start();
    expect(construct).toHaveBeenCalledOnce();
    const [venue, json] = construct.mock.calls[0];
    expect(venue).toBe('apexomni');
    // The options blob is uniform across venues; these are the fields this venue reads.
    expect(JSON.parse(json)).toMatchObject({
      key: 'api-key',
      secret: 'api-secret',
      passphrase: 'api-passphrase',
      omniKeySeed: '11'.repeat(65),
      testnet: true,
      publicDataOnly: false,
      remoteSigning: false,
      enableSpotMarkets: false,
      publicFeed: { mode: 'direct' },
      accountStartup: true,
    });
  });

  it('honors one plain caller ID without changing its signed value', async () => {
    const { exchange, commands, result } = setup();
    await exchange.start();
    result.mockReturnValue(['42']);
    const opts = placement({ clientOrderId: 'caller-native-42' });
    await expect(
      exchange.placeOrderWithClientOrderId(opts as PlaceOrderOpts & { clientOrderId: string }),
    ).resolves.toEqual({ orderIds: ['42'], clientOrderId: 'caller-native-42' });
    expect(commands).toEqual([{ op: 'placeOrders', orders: [opts], command: 1 }]);
  });

  it.each([
    { amount: 11 },
    { stopLoss: 90 },
    { takeProfit: 110 },
    { type: OrderType.StopLoss, triggerPrice: 90 },
    { clientOrderId: 'has whitespace' },
  ])('rejects an unhonorable caller ID shape before any host command: %j', async (override) => {
    const { exchange, commands } = setup();
    await exchange.start();
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    await expect(
      exchange.placeOrders([placement(), placement({ clientOrderId: 'caller', ...override })]),
    ).rejects.toThrow();
    expect(commands).toEqual([]);
  });

  it('rejects duplicate caller IDs across the whole batch before invoking WASM', async () => {
    const { exchange, commands } = setup();
    await exchange.start();
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    await expect(
      exchange.placeOrders([placement({ clientOrderId: 'same' }), placement({ clientOrderId: 'same' })]),
    ).rejects.toThrow(/duplicated/);
    expect(commands).toEqual([]);
  });

  it.each([
    { from: 1_700_000_000.123, to: 1_700_000_001.456 },
    { from: 1_700_000_000_123.9, to: 1_700_000_001_456.9 },
    { from: 1_700_000_000.123, to: 1_700_000_001_456.9 },
  ])('normalizes historical fill ranges to exact integer milliseconds: %j', async ({ from, to }) => {
    const { exchange, commands } = setup();
    await exchange.start();
    await exchange.fetchHistoricalFills({ symbol: MARKET.symbol, from, to, limit: 7 });
    expect(commands).toEqual([
      { op: 'fetchFills', symbol: MARKET.symbol, from: 1_700_000_000_123, to: 1_700_000_001_456, limit: 7, command: 1 },
    ]);
  });

  it('retains ApeX’s strict millisecond cutoff and epoch zero', async () => {
    const { exchange, commands } = setup();
    await exchange.start();
    await exchange.fetchHistoricalFills({ symbol: MARKET.symbol, from: 0, to: 1_000_000_000_000 });
    expect(commands).toEqual([
      { op: 'fetchFills', symbol: MARKET.symbol, from: 0, to: 1_000_000_000_000_000, command: 1 },
    ]);
  });

  it('keeps unsupported income and order history branded without issuing reads', async () => {
    const { exchange, commands } = setup();
    await exchange.start();
    expect(exchange.capabilities).toMatchObject({ orderHistory: false, incomeHistory: false, incomeRange: false });
    await expect(exchange.fetchHistoricalOrders()).resolves.toEqual([]);
    await expect(exchange.fetchIncomeHistory()).resolves.toEqual([]);
    await expect(exchange.fetchIncomeHistoryRange(0, 1000)).resolves.toEqual([]);
    expect(commands).toEqual([]);
  });

  it('creates a host for native account identity without starting venue streams', async () => {
    const { exchange, commands, result, start } = setup();
    result.mockReturnValue({ userId: '611779989544632629', affiliateId: null });
    await expect(exchange.getAccount()).resolves.toEqual({ userId: '611779989544632629', affiliateId: null });
    expect(commands).toEqual([{ op: 'fetchAccount', command: 1 }]);
    expect(start).not.toHaveBeenCalled();
  });

  it('fetches identity without an Omni seed or loaded account while sharing concurrent startup', async () => {
    const { exchange, store, commands, result, construct, start, setNativeLoaded } = setup(
      { omniKeySeed: undefined }, { balance: false, orders: false, positions: false },
    );
    const startup = exchange.start();
    void startup.catch(() => {});
    result.mockReturnValue({ userId: 'account-id', affiliateId: null });
    const account = exchange.getAccount();
    await vi.waitFor(() => expect(construct).toHaveBeenCalledOnce());
    await expect(account).resolves.toEqual({ userId: 'account-id', affiliateId: null });
    expect(commands).toEqual([{ op: 'fetchAccount', command: 1 }]);
    expect(store.loaded.balance).toBe(false);
    expect(store.loaded.positions).toBe(false);
    expect(JSON.parse(construct.mock.calls[0][1])).not.toHaveProperty('omniKeySeed');
    await expect(exchange.getAccount()).resolves.toEqual({ userId: 'account-id', affiliateId: null });
    expect(commands).toHaveLength(2);
    setNativeLoaded({ balance: true, orders: true, positions: true });
    await startup;
    expect(construct).toHaveBeenCalledOnce();
    expect(start).toHaveBeenCalledOnce();
  });

  it('fetches identity with API credentials while every account section remains unloaded', async () => {
    const { exchange, store, commands, result, start } = setup(
      { omniKeySeed: undefined }, { markets: false, tickers: false, balance: false, orders: false, positions: false },
    );
    result.mockReturnValue({ userId: 'account-id', affiliateId: null });
    await expect(exchange.getAccount()).resolves.toEqual({ userId: 'account-id', affiliateId: null });
    expect(commands).toEqual([{ op: 'fetchAccount', command: 1 }]);
    expect(store.loaded.balance).toBe(false);
    expect(start).not.toHaveBeenCalled();
  });

  it('propagates startup failure from identity lookup without dispatching a command', async () => {
    const { exchange, commands, construct } = setup();
    const failure = new Error('WASM load failed');
    setWasmEngineLoader(async () => {
      throw failure;
    });
    await expect(exchange.getAccount()).rejects.toBe(failure);
    expect(construct).not.toHaveBeenCalled();
    expect(commands).toEqual([]);
  });

  it('starts account validation after an identity-only host was created', async () => {
    const { exchange, store, construct, start, setNativeLoaded } = setup({}, { balance: false });
    await exchange.getAccount();
    expect(start).not.toHaveBeenCalled();
    const validation = exchange.validateAccount();
    await vi.waitFor(() => expect(start).toHaveBeenCalledOnce());
    setNativeLoaded({ balance: true });
    await expect(validation).resolves.toEqual({ error: '', balance: store.balance });
    expect(construct).toHaveBeenCalledOnce();
  });

  it('rejects identity lookup when disposed while the loader is pending', async () => {
    const { exchange, commands } = setup();
    setWasmEngineLoader(() => new Promise<WasmEngineModule>(() => {}));
    const account = expect(exchange.getAccount()).rejects.toThrow('disposed during startup');
    exchange.dispose();
    await account;
    expect(commands).toEqual([]);
  });
});
