import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeOptions, Market, PlaceOrderOpts } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { isBaseCapabilityFallback } from '../../exchanges/base';
import { isRustVenue, RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

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
const fixture = (options: Partial<ExchangeOptions> = {}) => {
  const result = vi.fn((_command: Record<string, unknown>): unknown => []);
  const commands: Array<Record<string, unknown>> = [];
  const noActions = () => '[]';
  const engine: WasmEngineLike = {
    start: noActions,
    command: (json) => {
      const command = JSON.parse(json);
      if (command.op === 'setTickerSymbols') return noActions();
      commands.push(command);
      return JSON.stringify([{ op: 'done', command: command.command, result: JSON.stringify(result(command)) }]);
    },
    takeFrame: () => undefined,
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
  store.update({
    markets: [MARKET],
    loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
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
  return { exchange, construct, commands, result };
};

describe('Rust ApeX Omni SDK boundary', () => {
  const exchanges: RustEngineExchange[] = [];
  const setup = (options: Partial<ExchangeOptions> = {}) => {
    const f = fixture(options);
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
    expect(JSON.parse(json)).toEqual({
      key: 'api-key',
      secret: 'api-secret',
      passphrase: 'api-passphrase',
      omniKeySeed: '11'.repeat(65),
      testnet: true,
      publicDataOnly: false,
      enableSpotMarkets: false,
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
    expect(isBaseCapabilityFallback(exchange.fetchHistoricalOrders, 'fetchHistoricalOrders')).toBe(true);
    expect(isBaseCapabilityFallback(exchange.fetchIncomeHistory, 'fetchIncomeHistory')).toBe(true);
    expect(isBaseCapabilityFallback(exchange.fetchIncomeHistoryRange, 'fetchIncomeHistoryRange')).toBe(true);
    await expect(exchange.fetchHistoricalOrders()).resolves.toEqual([]);
    await expect(exchange.fetchIncomeHistory()).resolves.toEqual([]);
    await expect(exchange.fetchIncomeHistoryRange(0, 1000)).resolves.toEqual([]);
    expect(commands).toEqual([]);
  });

  it('returns the native account ID through the getAccount host command', async () => {
    const { exchange, commands, result } = setup();
    await exchange.start();
    result.mockReturnValue({ userId: '611779989544632629', affiliateId: null });
    await expect(exchange.getAccount()).resolves.toEqual({ userId: '611779989544632629', affiliateId: null });
    expect(commands).toEqual([{ op: 'getAccount', command: 1 }]);
  });
});
