import { describe, expect, it } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { OrderSide, OrderStatus, OrderType } from '../../types';
import { nativeFixture } from '../cutover/nativeFixture';
import { BinanceSpotExchange } from '../reference/binance/binancespot.exchange';
import { BinancePrivateWebsocket } from '../reference/binance/binancespot.ws-private';

const createSubject = () => {
  const store = new FrameStore();
  const exchange = new BinanceSpotExchange('binancespot', { key: 'key', secret: 'secret' }, store);
  return { exchange, store, websocket: new BinancePrivateWebsocket(exchange) };
};

describe('Binance Spot official private stream wire shape', () => {
  it('reads executionReport fields from the top-level event', () => {
    const { store, websocket } = createSubject();

    websocket.handleOrderEvents([
      {
        e: 'executionReport',
        E: 1499405658658,
        s: 'ETHBTC',
        c: 'myOrder1',
        S: 'BUY',
        o: 'LIMIT',
        x: 'NEW',
        X: 'NEW',
        q: '1.00000000',
        p: '0.10264410',
        sp: '0.00000000',
        z: '0.00000000',
        l: '0.00000000',
      },
    ]);

    expect(store.orders).toMatchObject([
      {
        id: 'myOrder1',
        symbol: 'ETHBTC',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: OrderStatus.Open,
        amount: 1,
        price: 0.1026441,
      },
    ]);
  });

  it('reads outboundAccountPosition balances from the top-level B array', () => {
    const { store, websocket } = createSubject();
    store.apply(
      nativeFixture(
        {
          tickers: [
            {
              id: 'BTC/USDT',
              symbol: 'BTCUSDT',
              bid: 40_000,
              ask: 40_000,
              last: 40_000,
              mark: 40_000,
              index: 40_000,
              percentage: 0,
              fundingTime: 0,
              fundingRate: 0,
              volume: 0,
              quoteVolume: 0,
              openInterest: 0,
            },
          ],
        },
        store.markets.map((row) => row.symbol),
      ),
    );

    websocket.handleAccountEvents([
      {
        e: 'outboundAccountPosition',
        E: 1499405658849,
        u: 1499405658848,
        B: [
          { a: 'BTC', f: '0.10000000', l: '0.02000000' },
          { a: 'USDT', f: '100.00000000', l: '10.00000000' },
        ],
      },
    ]);

    expect(store.balance.currencies?.BTC).toMatchObject({ free: 0.1, used: 0.02 });
    expect(store.balance.currencies?.BTC.total).toBeCloseTo(0.12);
    expect(store.balance.currencies?.USDT).toMatchObject({ free: 100, used: 10, total: 110 });
    expect(store.balance).toMatchObject({ free: 4100, used: 810, total: 4910 });
  });
});
