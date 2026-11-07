import { afterEach, describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../reference/binance/binance.exchange';
import { DefaultStore } from '../../store/store.base';
import { type Market, type Position, PositionMode, PositionSide } from '../../types';
import { createPositionId } from '../../utils/createPositionId';

const create = (mode: PositionMode) => {
  const store = new DefaultStore();
  store.update({
    markets: [{ symbol: 'BTCUSDT', limits: { leverage: { min: 1, max: 125 } } } as Market],
    positions: [PositionSide.Long, PositionSide.Short].slice(0, mode === PositionMode.OneWay ? 1 : 2).map((side) => ({
      id: createPositionId('BTCUSDT', side, mode), symbol: 'BTCUSDT', side, positionMode: mode,
      leverage: 20, contracts: 0,
    } as Position)),
  });
  return new BinanceExchange('binance', { key: 'fake-key', secret: 'fake-secret' }, store);
};

afterEach(() => vi.restoreAllMocks());

describe('Binance leverage lifecycle', () => {
  it.each([PositionMode.OneWay, PositionMode.Hedged])('restores the original leverage after lowering it in %s', async (mode) => {
    const exchange = create(mode);
    const post = vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: {} });
    await exchange.setLeverage('BTCUSDT', 19);
    expect(exchange.store.positions.every((p) => p.leverage === 19)).toBe(true);
    await exchange.setLeverage('BTCUSDT', 20);
    expect(post.mock.calls.map((call) => call[1])).toEqual([{ symbol: 'BTCUSDT', leverage: 19 }, { symbol: 'BTCUSDT', leverage: 20 }]);
    expect(exchange.store.positions.every((p) => p.leverage === 20)).toBe(true);
    exchange.dispose();
  });

  it('propagates a rejected leverage write without changing cached leverage', async () => {
    const exchange = create(PositionMode.OneWay);
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    vi.spyOn(exchange.xhr, 'post').mockRejectedValue(new Error('scripted rejection'));
    await expect(exchange.setLeverage('BTCUSDT', 19)).rejects.toThrow('scripted rejection');
    expect(exchange.store.positions[0].leverage).toBe(20);
    exchange.dispose();
  });
});
