import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

describe('Rust WOO execution history boundary', () => {
  const exchanges: RustEngineExchange[] = [];
  const setup = () => {
    const exchange = new RustEngineExchange('woo', { key: 'fixture', secret: 'fixture' }, new DefaultStore());
    exchanges.push(exchange);
    const command = vi.fn().mockResolvedValue([]);
    // Observe the engine boundary without loading a built WASM artifact.
    Object.assign(exchange, { host: { command, dispose: vi.fn() } });
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
});
