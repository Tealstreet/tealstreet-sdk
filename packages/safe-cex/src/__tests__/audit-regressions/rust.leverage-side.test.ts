import type { Position } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { PositionSide } from '../../types';

describe('Rust leverage command side boundary', () => {
  const exchanges: RustEngineExchange[] = [];
  afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

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
    await exchange.setAllLeverage(8);
    expect(command.mock.calls.map(([body]) => body)).toEqual([
      { op: 'setLeverage', symbol: 'BTCUSDT', leverage: 8, side: PositionSide.Long },
      { op: 'setLeverage', symbol: 'BTCUSDT', leverage: 8, side: PositionSide.Short },
    ]);
  });
});
