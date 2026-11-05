vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { ExchangeName, Market, Position } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { PositionSide } from '../../types';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

describe('Rust leverage command side boundary', () => {
  const exchanges: RustEngineExchange[] = [];
  afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

  // Venues answering leverage reads from their published rails. BingX, WEEX and
  // Bitget read the venue; their crates test per-leg reads and write fencing.
  const independentVenues: ExchangeName[] = ['mexc', 'phemex'];
  it.each(independentVenues)('keeps confirmed %s leg settings when leverage is read', async (name) => {
    const store = new DefaultStore();
    const market: Market = {
      id: 'BTCUSDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.001, price: 0.1 },
      limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
      info: {},
    };
    store.update({ markets: [market], fetchedLeverage: { BTCUSDT: { buyLeverage: 5, sellLeverage: 7 } } });
    const exchange = new RustEngineExchange(name, { key: '', secret: '' }, store);
    exchanges.push(exchange);
    adoptEngineCapabilities(exchange);
    await expect(exchange.fetchLeverageForSymbol('BTCUSDT')).resolves.toMatchObject({
      buyLeverage: 5,
      sellLeverage: 7,
    });
    expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 5, sellLeverage: 7 });
  });

  it.each([undefined, PositionSide.Long, PositionSide.Short, '' as PositionSide])(
    'forwards the requested side %s unchanged',
    async (side) => {
      const exchange = new RustEngineExchange('bitget', { key: '', secret: '' }, new DefaultStore());
      exchanges.push(exchange);
      const command = vi.fn().mockResolvedValue(null);
      Object.defineProperty(exchange, 'command', { value: command });
      await exchange.setLeverage('BTCUSDT', 8, side);
      expect(command).toHaveBeenCalledExactlyOnceWith({
        op: 'setLeverage',
        symbol: 'BTCUSDT',
        leverage: 8,
        ...(side !== undefined ? { side } : {}),
      });
    },
  );

  it.each([undefined, 'BTCUSDT'])('forwards position mode scope %s', async (symbol) => {
    const exchange = new RustEngineExchange('phemex', { key: '', secret: '' }, new DefaultStore());
    exchanges.push(exchange);
    const command = vi.fn().mockResolvedValue(null);
    Object.defineProperty(exchange, 'command', { value: command });
    await exchange.changePositionMode(true, symbol);
    expect(command).toHaveBeenCalledExactlyOnceWith({
      op: 'setPositionMode',
      hedged: true,
      ...(symbol !== undefined ? { symbol } : {}),
    });
  });

  it('sets leverage for both open hedge legs without dropping their sides', async () => {
    const store = new DefaultStore();
    store.update({
      positions: [
        { id: 'long', symbol: 'BTCUSDT', side: PositionSide.Long },
        { id: 'short', symbol: 'BTCUSDT', side: PositionSide.Short },
      ] as Position[],
    });
    const exchange = new RustEngineExchange('bingx', { key: '', secret: '' }, store);
    exchanges.push(exchange);
    const command = vi.fn().mockResolvedValue(null);
    Object.defineProperty(exchange, 'command', { value: command });
    // Positions come from the authoritative native read; this test pins the legs' sides.
    Object.defineProperty(exchange, 'fetchPositions', { value: async () => store.positions });
    await exchange.setAllLeverage(8);
    expect(command.mock.calls.map(([body]) => body)).toEqual([
      { op: 'setLeverage', symbol: 'BTCUSDT', leverage: 8, side: PositionSide.Long },
      { op: 'setLeverage', symbol: 'BTCUSDT', leverage: 8, side: PositionSide.Short },
    ]);
  });
});
