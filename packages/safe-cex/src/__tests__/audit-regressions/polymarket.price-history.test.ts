import { describe, expect, it, vi } from 'vitest';
import { PolymarketExchange } from '../reference/polymarket/polymarket.exchange';
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
    expect(history).toHaveBeenLastCalledWith('123', undefined, undefined, 'max', 240);
    history.mockClear();
    for (const opts of [{ from: -1 }, { from: 200, to: 100 }, { limit: 0 }]) {
      await expect(exchange.fetchOHLCV({ symbol: 'prediction-Yes', interval: '1m', ...opts })).rejects.toThrow('Invalid');
    }
    expect(history).not.toHaveBeenCalled();
    exchange.dispose();
  });
  it.each([
    { bounds: {}, selector: { interval: 'max' }, timestamps: [102000, 103000] },
    { bounds: { from: 1000 }, selector: { startTs: 1 }, timestamps: [102000, 103000] },
    { bounds: { from: 100500 }, selector: { startTs: 100 }, timestamps: [102000, 103000] },
    { bounds: { from: 100500, to: 102000 }, selector: { startTs: 100, endTs: 102 }, timestamps: [101000, 102000] },
  ])('supplies a valid venue time selector for $bounds', async ({ bounds, selector, timestamps }) => {
    const exchange = setup();
    const get = vi.fn().mockImplementation(async (_path, { params }) => {
      if ((params.startTs === undefined && params.interval === undefined) ||
          (params.endTs !== undefined && params.startTs === undefined)) {
        throw new Error('invalid filters: the time component is mandatory');
      }
      return { data: { history: [{ t: 100, p: 0.1 }, { t: 101, p: 0.2 }, { t: 102, p: 0.3 }, { t: 103, p: 0.4 }] } };
    });
    (exchange as any).api.xhr = { get };
    const candles = await exchange.fetchOHLCV({ symbol: 'prediction-Yes', interval: '5m', limit: 2, ...bounds });
    expect(get).toHaveBeenCalledWith('/prices-history', { params: {
      market: '123', startTs: undefined, endTs: undefined, interval: undefined, fidelity: 5, ...selector,
    } });
    expect(candles.map((candle) => candle.timestamp)).toEqual(timestamps);
    exchange.dispose();
  });
  it('rejects upper-only bounds before transport instead of truncating old history to the max window', async () => {
    const exchange = setup();
    const history = vi.fn();
    (exchange as any).api.getPriceHistory = history;
    await expect(exchange.fetchOHLCV({ symbol: 'prediction-Yes', interval: '1m', to: 102000 }))
      .rejects.toThrow('Polymarket upper-only candle bounds require from');
    expect(history).not.toHaveBeenCalled();
    exchange.dispose();
  });
  it.each([0, 999.999])('rejects from=%s before transport because the API treats startTs=0 as absent', async (from) => {
    const exchange = setup();
    const history = vi.fn();
    (exchange as any).api.getPriceHistory = history;
    for (const to of [undefined, 102000]) {
      await expect(exchange.fetchOHLCV({ symbol: 'prediction-Yes', interval: '1m', from, to }))
        .rejects.toThrow('Polymarket candle from must be at least 1000 milliseconds');
    }
    expect(history).not.toHaveBeenCalled();
    exchange.dispose();
  });
});
