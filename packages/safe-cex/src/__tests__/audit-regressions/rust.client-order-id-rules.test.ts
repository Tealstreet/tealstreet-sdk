import type { ExchangeName, Market, PlaceOrderOpts } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { isRustVenue, RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

const MARKET: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 10 }, leverage: { min: 1, max: 100 } },
  info: {},
};
const order = (extra: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 1,
  price: 69_000,
  ...extra,
});
const exchanges: RustEngineExchange[] = [];
const setup = (name: ExchangeName) => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  const exchange = new RustEngineExchange(name, { key: 'fixture', secret: 'fixture' }, store);
  const command = vi.fn().mockResolvedValue(['native-order']);
  // Record the actual engine-command boundary without starting any transport.
  Object.assign(exchange, { host: { command: (input: unknown) => command(input), dispose: vi.fn() } });
  adoptEngineCapabilities(exchange);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  exchanges.push(exchange);
  return { exchange, command };
};
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

describe.each([
  ['binance', 36, 'x-s3vFFRQp'],
  ['binancepm', 32, 'x-s3vFFRQp'],
  ['phemex', 40, 'Tealstreet_'],
] as const)('Rust %s caller IDs', (name, limit, prefix) => {
  it('forwards exact bare IDs through both placement entry points, including the length boundary', async () => {
    const { exchange, command } = setup(name);
    const single = order({ clientOrderId: 'bare.id:/A_-' });
    await expect(exchange.placeOrder(single)).resolves.toEqual(['native-order']);
    expect(command).toHaveBeenLastCalledWith({ op: 'placeOrders', orders: [single] });
    const batch = [order({ clientOrderId: 'A'.repeat(limit) }), order({ type: OrderType.Market, clientOrderId: 'bare-second' })];
    await expect(exchange.placeOrders(batch)).resolves.toEqual(['native-order']);
    expect(command).toHaveBeenLastCalledWith({ op: 'placeOrders', orders: batch });
    expect(command).toHaveBeenCalledTimes(2);
    expect(single.clientOrderId).toBe('bare.id:/A_-');
    expect(batch[0].clientOrderId).toBe('A'.repeat(limit));
  });

  it('rejects empty and overlong IDs before any engine command, including a later invalid batch item', async () => {
    const { exchange, command } = setup(name);
    for (const clientOrderId of ['', 'a'.repeat(limit + 1)]) {
      await expect(exchange.placeOrder(order({ clientOrderId }))).rejects.toThrow(/clientOrderId must/);
      await expect(exchange.placeOrders([order({ clientOrderId: 'valid-first' }), order({ clientOrderId })])).rejects.toThrow(/clientOrderId must/);
    }
    expect(command).not.toHaveBeenCalled();
  });

  it('rejects duplicate IDs and shapes or sizes that require extra native orders atomically', async () => {
    const { exchange, command } = setup(name);
    await expect(exchange.placeOrders([
      order({ clientOrderId: 'same' }), order({ clientOrderId: 'same' }),
    ])).rejects.toThrow(/duplicated/);
    for (const extra of [
      { type: OrderType.StopLoss, triggerPrice: 68_000 },
      { stopLoss: 68_000 },
      { amount: 11 },
    ]) {
      await expect(exchange.placeOrders([
        order({ clientOrderId: 'valid-first' }), order({ ...extra, clientOrderId: 'invalid-shape' }),
      ])).rejects.toThrow(/clientOrderId/);
    }
    expect(command).not.toHaveBeenCalled();
  });

  it('mints the existing broker prefix and forwards it verbatim', async () => {
    const { exchange, command } = setup(name);
    const clientOrderId = exchange.mintClientOrderId(order());
    expect(clientOrderId?.startsWith(prefix)).toBe(true);
    expect(clientOrderId!.length).toBeLessThanOrEqual(limit);
    expect(command).not.toHaveBeenCalled();
    await exchange.placeOrder(order({ clientOrderId }));
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'placeOrders', orders: [order({ clientOrderId })] });
  });
});

it('keeps the venue-specific character rules', async () => {
  const phemex = setup('phemex');
  await phemex.exchange.placeOrder(order({ clientOrderId: 'caller ID 漢' }));
  expect(phemex.command).toHaveBeenCalledExactlyOnceWith({
    op: 'placeOrders', orders: [order({ clientOrderId: 'caller ID 漢' })],
  });
  for (const name of ['binance', 'binancepm'] as const) {
    const { exchange, command } = setup(name);
    await expect(exchange.placeOrder(order({ clientOrderId: 'caller ID' }))).rejects.toThrow(/clientOrderId must/);
    expect(command).not.toHaveBeenCalled();
  }
});

it('continues rejecting Lighter caller IDs through both placement entry points before any command', async () => {
  const { exchange, command } = setup('lighter');
  await expect(exchange.placeOrder(order({ clientOrderId: 'caller' }))).rejects.toThrow(/cannot honor a caller clientOrderId/);
  await expect(exchange.placeOrders([order(), order({ clientOrderId: 'caller' })])).rejects.toThrow(/cannot honor a caller clientOrderId/);
  expect(exchange.mintClientOrderId(order())).toBeUndefined();
  expect(command).not.toHaveBeenCalled();
});

describe('Rust spot product selection and inherited capabilities', () => {
  it.each(['hyperliquidspot', 'krakenspot'] as const)('%s retains spot identity and branded history fallbacks', async (name) => {
    const { exchange, command } = setup(name);
    expect(isRustVenue(name)).toBe(true);
    expect(exchange.isSpot).toBe(true);
    expect(exchange.capabilities).toMatchObject({
      fillHistory: false,
      orderHistory: false,
      incomeHistory: false,
      incomeRange: false,
    });
    await expect(exchange.fetchHistoricalFills({})).resolves.toEqual([]);
    await expect(exchange.fetchHistoricalOrders()).resolves.toEqual([]);
    await expect(exchange.fetchIncomeHistory()).resolves.toEqual([]);
    await expect(exchange.fetchIncomeHistoryRange(0, 1000)).resolves.toEqual([]);
    expect(command).not.toHaveBeenCalled();
  });

  it('honors the Hyperliquid spot bracket entry cloid and rejects duplicate batch IDs atomically', async () => {
    const { exchange, command } = setup('hyperliquidspot');
    const clientOrderId = `0x${'12'.repeat(16)}`;
    const bracket = order({ clientOrderId, stopLoss: 60_000, takeProfit: 80_000 });
    await expect(exchange.placeOrder(bracket)).resolves.toEqual(['native-order']);
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'placeOrders', orders: [bracket] });
    command.mockClear();
    await expect(exchange.placeOrders([order({ clientOrderId }), order({ clientOrderId })])).rejects.toThrow(/duplicated/);
    await expect(exchange.placeOrder(order({ clientOrderId: clientOrderId.toUpperCase() }))).rejects.toThrow(/clientOrderId/);
    expect(command).not.toHaveBeenCalled();
  });

  it('rejects a Kraken spot caller ID in a later batch row before the engine receives any write', async () => {
    const { exchange, command } = setup('krakenspot');
    await expect(exchange.placeOrders([order(), order({ clientOrderId: 'caller' })])).rejects.toThrow(/cannot honor a caller clientOrderId/);
    expect(exchange.mintClientOrderId(order())).toBeUndefined();
    expect(command).not.toHaveBeenCalled();
  });
});
