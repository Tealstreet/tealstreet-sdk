vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import { afterEach, describe, expect, it, vi } from 'vitest';

import { isRustVenue, RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

describe('Rust WOO execution history boundary', () => {
  const exchanges: RustEngineExchange[] = [];
  const setup = () => {
    const exchange = new RustEngineExchange('woo', { key: 'fixture', secret: 'fixture' }, new DefaultStore());
    exchanges.push(exchange);
    const command = vi.fn().mockResolvedValue([]);
    // Observe the engine boundary without loading a built WASM artifact.
    Object.assign(exchange, { host: { command: (input: unknown) => command(input), dispose: vi.fn() } });
    adoptEngineCapabilities(exchange);
    return { exchange, command };
  };
  afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

  it.each([
    { from: 1_700_000_000.123, to: 1_700_000_001.456 },
    { from: 1_700_000_000_123.9, to: 1_700_000_001_456.9 },
    { from: 1_700_000_000.123, to: 1_700_000_001_456.9 },
  ])('converts second, millisecond and mixed ranges before the engine: %j', async ({ from, to }) => {
    const { exchange, command } = setup();
    await exchange.fetchHistoricalFills({ symbol: 'woo:BTCUSDT.P', from, to, limit: 2 });
    expect(command).toHaveBeenCalledExactlyOnceWith({
      op: 'fetchFills',
      symbol: 'woo:BTCUSDT.P',
      from: 1_700_000_000_123,
      to: 1_700_000_001_456,
      limit: 2,
    });
  });

  it('retains epoch zero and the inclusive millisecond cutoff', async () => {
    const { exchange, command } = setup();
    await exchange.fetchHistoricalFills({ from: 0, to: 1_000_000_000_000 });
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'fetchFills', from: 0, to: 1_000_000_000_000 });
  });

  it.each([undefined, -1, NaN, Infinity])('omits an invalid execution range bound: %s', async (value) => {
    const { exchange, command } = setup();
    await exchange.fetchHistoricalFills({ from: value, to: value });
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'fetchFills', from: undefined, to: undefined });
  });

  it('registers WOO beside Orderly brokers and preserves caller IDs and slippage through placement', async () => {
    expect(isRustVenue('woo')).toBe(false);
    expect(isRustVenue('woofi')).toBe(false);
    expect(isRustVenue('kodiakfi')).toBe(false);
    const { exchange, command } = setup();
    const opts = { symbol: 'BTCUSDT', type: OrderType.Market, side: OrderSide.Buy, amount: 1,
      maxSlippagePct: 0.25, clientOrderId: '9007199254740991' };
    await expect(exchange.placeOrderWithClientOrderId(opts)).resolves.toEqual({
      orderIds: [], clientOrderId: opts.clientOrderId,
    });
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'placeOrders', orders: [opts] });
  });

  it('rejects invalid caller IDs and non-finite slippage before any mixed batch command', async () => {
    const { exchange, command } = setup();
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    const opts = { symbol: 'BTCUSDT', type: OrderType.Market, side: OrderSide.Buy, amount: 1 };
    for (const clientOrderId of ['0', '01', '-1', '1.0', '9007199254740992']) {
      await expect(exchange.placeOrders([opts, { ...opts, clientOrderId }])).rejects.toThrow('clientOrderId must');
    }
    for (const maxSlippagePct of [NaN, Infinity, -Infinity]) {
      await expect(exchange.placeOrders([opts, { ...opts, maxSlippagePct }])).rejects.toThrow('must be finite');
    }
    for (const field of ['bybitBbo', 'bybitSlippageToleranceTicks']) {
      await expect(exchange.placeOrders([opts, { ...opts, [field]: undefined }])).rejects.toThrow(
        'does not support Bybit BBO or TickSize controls',
      );
    }
    expect(command).not.toHaveBeenCalled();
  });

  it('sends account-wide cancellation to the native venue without a store-order scan', async () => {
    const { exchange, command } = setup();
    await exchange.cancelAllOrders();
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'cancelSymbolOrders', symbol: '' });
  });
});
