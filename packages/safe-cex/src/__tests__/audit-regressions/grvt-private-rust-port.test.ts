import { describe, expect, it, vi } from 'vitest';
import { GrvtExchange } from '../reference/grvt/grvt.exchange';
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

describe('GRVT session resend headers', () => {
  it('refreshes the SDK cookie together with its proxy session header', async () => {
    const { createTradingAPI } = await import('../reference/grvt/grvt.api');
    const owner = { apiExtras: { sessionCookie: 'session-before', accountId: 'account' }, log: vi.fn(),
      authenticate: vi.fn(async () => { owner.apiExtras.sessionCookie = 'session-after'; return true; }) };
    const api = createTradingAPI({} as any, owner);
    let attempts = 0;
    api.defaults.adapter = async (config) => {
      if (attempts++ === 0) {
        throw Object.assign(new Error('Session expired'), { config, response: { status: 401, data: { c: 1000, m: 'Session expired' } } });
      }
      expect(config.headers.get('Cookie')).toBe('rm=true; gravity=session-after');
      expect(config.headers.get('X-Ts-Grvt-Session')).toBe('session-after');
      return { config, data: { r: { a: true } }, status: 200, statusText: 'OK', headers: {} };
    };
    await api.post('/lite/v1/cancel_order', { sa: '777', oi: '0x123' }, { headers: { Cookie: 'gravity=session-before' } });
    expect(attempts).toBe(2);
    expect(owner.authenticate).toHaveBeenCalledOnce();
  });
});
