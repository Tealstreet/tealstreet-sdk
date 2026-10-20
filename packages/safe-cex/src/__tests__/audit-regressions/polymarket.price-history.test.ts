import { describe, expect, it, vi } from 'vitest';
import { PolymarketExchange } from '../../exchanges/polymarket/polymarket.exchange';
import { DefaultStore } from '../../store/store.base';

describe('Polymarket price history uses raw API minutes and caller milliseconds', () => {
  const setup = () => {
    const exchange = new PolymarketExchange('polymarket', { publicDataOnly: true } as any, new DefaultStore());
    (exchange as any).symbolToToken.set('prediction-Yes', '123');
    return exchange;
  };
  it('uses one-minute fidelity and filters returned samples to precise inclusive bounds', async () => {
    const exchange = setup();
    const history = vi.fn().mockResolvedValue([{ t: 100, p: '0.1' }, { t: 101, p: '0.2' }, { t: 102, p: '0.3' }, { t: 103, p: '0.4' }]);
    (exchange as any).api.getPriceHistory = history;
    const candles = await exchange.fetchOHLCV({ symbol: 'prediction-Yes', interval: '1m', from: 100500, to: 102000, limit: 1 });
    expect(history).toHaveBeenCalledWith('123', 100, 102, undefined, 1);
    expect(candles).toEqual([{ timestamp: 102000, open: 0.3, high: 0.3, low: 0.3, close: 0.3, volume: 0 }]);
    exchange.dispose();
  });
  it('uses interval-minute fidelity and rejects invalid requests before transport', async () => {
    const exchange = setup();
    const history = vi.fn().mockResolvedValue([]);
    (exchange as any).api.getPriceHistory = history;
    await exchange.fetchOHLCV({ symbol: 'prediction-Yes', interval: '4h' });
    expect(history).toHaveBeenLastCalledWith('123', undefined, undefined, undefined, 240);
    history.mockClear();
    for (const opts of [{ from: -1 }, { from: 200, to: 100 }, { limit: 0 }]) {
      await expect(exchange.fetchOHLCV({ symbol: 'prediction-Yes', interval: '1m', ...opts })).rejects.toThrow('Invalid');
    }
    expect(history).not.toHaveBeenCalled();
    exchange.dispose();
  });
});
