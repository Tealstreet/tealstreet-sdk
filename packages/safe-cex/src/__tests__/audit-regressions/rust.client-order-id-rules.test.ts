import type { ExchangeName, Market, PlaceOrderOpts } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

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
  Object.assign(exchange, { host: { command, dispose: vi.fn() } });
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  exchanges.push(exchange);
  return { exchange, command };
};
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

describe.each([
  ['binance', 36, 'x-s3vFFRQp'],
  ['binancepm', 32, 'x-s3vFFRQp'],
  ['phemex', 40, 'Tealstreet_'],
  ['coinbase', 128, 'tealstreet-'],
  ['coinbasespot', 128, 'tealstreet-'],
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


it.each(['coinbase', 'coinbasespot'] as const)('%s client IDs preserve well-formed Unicode and reject native-invalid text before a batch command', async (name) => {
  const { exchange, command } = setup(name);
  const clientOrderId = '🦀'.repeat(128);
  await exchange.placeOrder(order({ clientOrderId }));
  expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'placeOrders', orders: [order({ clientOrderId })] });
  command.mockClear();
  for (const invalid of ['\0', '\u007f', '\u0085', '\ud800', '\udc00', 'a\ud800b', '🦀'.repeat(129)]) {
    await expect(exchange.placeOrder(order({ clientOrderId: invalid }))).rejects.toThrow(/clientOrderId/);
    await expect(exchange.placeOrders([order({ clientOrderId: 'valid-first' }), order({ clientOrderId: invalid })])).rejects.toThrow(/clientOrderId/);
  }
  expect(command).not.toHaveBeenCalled();
});
