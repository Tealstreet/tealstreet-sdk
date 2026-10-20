import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const exchanges: RustEngineExchange[] = [];
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

// The SDK caller-ID contract must also hold before a real WASM host is loaded.
describe.each([
  { venue: 'binance' as const, maxLength: 36 },
  { venue: 'binancepm' as const, maxLength: 32 },
])('Rust $venue caller-ID boundary', ({ venue, maxLength }) => {
  it('honors caller IDs and mints broker-attributed IDs through inherited helpers', async () => {
    const store = new DefaultStore();
    store.update({ markets: [{
      id: 'BTCUSDT', symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', active: true,
      precision: { amount: 0.001, price: 0.1 },
      limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 125 } },
    }] });
    const exchange = new RustEngineExchange(venue, { key: 'fixture', secret: 'fixture' }, store);
    exchanges.push(exchange);
    const command = vi.fn().mockResolvedValue(['native-order']);
    Object.assign(exchange, { host: { command, dispose: vi.fn() } });
    const opts = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 60000 };
    const clientOrderId = 'A'.repeat(maxLength);
    await expect(exchange.placeOrderWithClientOrderId({ ...opts, clientOrderId })).resolves.toEqual({
      orderIds: ['native-order'], clientOrderId,
    });
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'placeOrders', orders: [{ ...opts, clientOrderId }] });
    const minted = exchange.mintClientOrderId(opts);
    expect(minted).toMatch(/^x-s3vFFRQp[.A-Z:/a-z0-9_-]+$/);
    expect(minted!.length).toBeLessThanOrEqual(maxLength);
    command.mockClear();
    for (const invalid of ['', 'has space', 'é', 'A'.repeat(maxLength + 1)]) {
      await expect(exchange.placeOrder({ ...opts, clientOrderId: invalid })).rejects.toThrow('clientOrderId');
    }
    await expect(exchange.placeOrders([
      { ...opts, clientOrderId: 'same' }, { ...opts, clientOrderId: 'same' },
    ])).rejects.toThrow('duplicated');
    await expect(exchange.placeOrder({ ...opts, clientOrderId: 'split', amount: 101 })).rejects.toThrow('max size');
    expect(command).not.toHaveBeenCalled();
  });
});
