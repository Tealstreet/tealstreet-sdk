import { describe, expect, it } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { nativeMarket } from '../cutover/nativeMarket';
import { applyReferenceFixture } from '../cutover/referenceFixture';
import { GrvtExchange } from '../reference/grvt/grvt.exchange';

describe('GRVT public delta fields', () => {
  it('retains each rolling volume side and open price while last changes', () => {
    const store = new FrameStore();
    const exchange = new GrvtExchange('grvt', { publicDataOnly: true } as any, store);
    applyReferenceFixture(
      store,
      {
        markets: [nativeMarket({ id: 'BTC_USDT_Perp', symbol: 'BTC_USDT_PERP' }) as any],
        tickers: [{ id: 'BTC_USDT_Perp', symbol: 'BTC_USDT_PERP', last: 0 } as any],
      },
      store.markets.map((row) => row.symbol),
    );
    const update = (f: object) =>
      exchange.publicWebsocket.onMessage({
        data: JSON.stringify({ s: 'v1.ticker.d', f: { i: 'BTC_USDT_Perp', ...f } }),
      } as MessageEvent);
    update({ lp: '100', op: '80', bv: '2', sv: '3', oi: '4' });
    update({ bv: '4', oi: '', bb: '99' });
    expect(store.tickers[0]).toMatchObject({
      last: 100,
      volume: 7,
      quoteVolume: 700,
      percentage: 25,
      openInterest: 0,
      bid: 99,
    });
    update({ lp: '120', nf: 1_700_000_400_000_000_000 });
    expect(store.tickers[0]).toMatchObject({ volume: 7, quoteVolume: 840, percentage: 50, fundingTime: 1_700_000_400 });
  });
});
