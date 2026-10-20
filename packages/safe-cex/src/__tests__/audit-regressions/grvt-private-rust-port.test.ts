import { describe, expect, it, vi } from 'vitest';
import { GrvtExchange } from '../../exchanges/grvt/grvt.exchange';
import { DefaultStore } from '../../store/store.base';
import { MarginType } from '../../types';

describe('GRVT private native values', () => {
  it('answers the private server ping', () => {
    const e = new GrvtExchange('grvt', {} as any, new DefaultStore());
    e.privateWebsocket.trackLastReceived = vi.fn();
    e.privateWebsocket.send = vi.fn();
    e.privateWebsocket.onMessage({ data: 'ping' } as MessageEvent);
    expect(e.privateWebsocket.send).toHaveBeenCalledWith('pong');
  });
  it.each(['0', 0])('preserves explicitly zero available collateral (%s)', async (zero) => {
    const exchange = new GrvtExchange('grvt', {} as any, new DefaultStore());
    vi.spyOn(exchange.tradingXhr, 'post').mockResolvedValue({ data: { r: { te: '100', ts2: zero, tu: '0' } } } as any);
    await expect(exchange.fetchBalance()).resolves.toMatchObject({ total: 100, free: 0, used: 100 });
  });
  it('preserves CROSS margin and uses entry price rather than ep1 exit price', () => {
    const store = new DefaultStore();
    store.update({ markets: [{ id: 'BTC_USDT_Perp', symbol: 'BTC_USDT_PERP' } as any] });
    const exchange = new GrvtExchange('grvt', {} as any, store);
    const position = (exchange.privateWebsocket as any).parsePosition({ i: 'BTC_USDT_Perp', s: '1', ep: '100', ep1: '90', mt: 'CROSS' });
    expect(position).toMatchObject({ marginType: MarginType.Cross, entryPrice: 100, contracts: 1 });
  });
});
