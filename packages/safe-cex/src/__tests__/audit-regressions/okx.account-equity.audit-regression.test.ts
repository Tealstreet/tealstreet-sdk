import type { Market, Order, Position } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { parseAccountEquity } from '../reference/okx/okx.account-equity';
import { OKXExchange } from '../reference/okx/okx.exchange';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderStatus, OrderType, PositionMode, PositionSide } from '../../types';

const exchanges: OKXExchange[] = [];
const fixture = () => {
  const store = new DefaultStore();
  const market: Market = {
    id: 'BTC-USD-SWAP',
    symbol: 'BTCUSD',
    base: 'BTC',
    quote: 'USD',
    isInverse: true,
    active: true,
    contractSize: 100,
    precision: { price: 0.1, amount: 1 },
    limits: { amount: { min: 1, max: 1000 }, leverage: { min: 1, max: 100 } },
    info: { ctValCcy: 'USD', settleCcy: 'BTC' },
  };
  const position: Position = {
    id: 'BTCUSD',
    symbol: 'BTCUSD',
    side: PositionSide.Long,
    contracts: 0.0125,
    notional: 500,
    leverage: 10,
    entryPrice: 39000,
    unrealizedPnl: 12.8,
    liquidationPrice: 30000,
    marginType: MarginType.Cross,
    positionMode: PositionMode.OneWay,
    info: { instId: market.id, ccy: 'BTC', markPx: '40000', imr: '0.00125', upl: '0.0003205128205128205' },
  };
  const order: Order = {
    id: 'open',
    symbol: 'BTCUSD',
    type: OrderType.Limit,
    side: OrderSide.Buy,
    status: OrderStatus.Open,
    amount: 4,
    filled: 0.0025,
    remaining: 0.0075,
    price: 40000,
    reduceOnly: false,
    lastUpdate: 1,
    info: { ccy: 'BTC', lever: '10' },
  };
  store.update({
    markets: [market],
    tickers: [{ id: market.id, symbol: market.symbol, last: 40000, mark: 40000 } as never],
    positions: [position],
    orders: [order],
    balance: {
      used: 50,
      free: 40017,
      total: 40087.18,
      upnl: 12.82,
      info: [
        { ccy: 'BTC', eq: '1' },
        { ccy: 'USD', eq: '100' },
      ],
    },
    loaded: { markets: true, tickers: true, positions: true, balance: true, orders: true },
  });
  const exchange = new OKXExchange('okx', { key: 'key', secret: 'secret', passphrase: 'pass' }, store);
  exchange._accountLevel = 2;
  exchange._accountPositionMode = PositionMode.OneWay;
  vi.spyOn(exchange.xhr, 'get').mockRejectedValue(new Error('Unexpected network request'));
  exchanges.push(exchange);
  return { exchange, store };
};
afterEach(() => {
  exchanges.splice(0).forEach((e) => e.dispose());
  vi.useRealTimers();
});

describe('OKX account equity anchor', () => {
  it('keeps real USD and BTC equity and open-order margin across a local tick calculation', async () => {
    const { exchange, store } = fixture();
    exchange.privateWebsocket.handleAccountTopic({
      data: [{ totalEq: '40400', details: [{ ccy: 'USD', eq: '400' }] }],
    });
    expect(store.balance).toMatchObject({ used: 50, free: 40317, total: 40387.18, upnl: 12.82 });
    expect(store.balance.currencies?.USD).toMatchObject({ total: 400, free: 400 });
    expect(store.balance.currencies?.BTC).toMatchObject({ used: 0.002075, free: 0.997925 });
    const recomputed = await exchange.formatBalanceUsedAvail(
      [
        { ccy: 'BTC', eq: '1' },
        { ccy: 'USD', eq: '100' },
      ],
      store.positions,
      store.orders,
    );
    expect(recomputed).toEqual(store.balance);
  });
  it('preserves the new websocket balance if tick formatting completes after another account event', async () => {
    vi.useFakeTimers();
    const { exchange, store } = fixture();
    let fresh: typeof store.balance | undefined;
    const tick = exchange.tick();
    // Let the real formatter calculate first, then deliver WS before tick commits.
    queueMicrotask(() =>
      queueMicrotask(() => {
        exchange.applyAccountEquity([{ totalEq: '40400', details: [{ ccy: 'USD', eq: '400' }] }]);
        fresh = store.balance;
      }),
    );
    await tick;
    expect(store.balance).toBe(fresh);
  });
  it('rechecks websocket equity at the final startup snapshot commit', async () => {
    vi.useFakeTimers();
    const { exchange, store } = fixture();
    const stale = store.balance;
    const writes = vi.spyOn(store, 'update');
    let beforeWs = -1;
    vi.spyOn(exchange, 'fetchMarkets').mockResolvedValue(store.markets);
    vi.spyOn(exchange, 'fetchTickersRaw').mockResolvedValue([
      {
        instId: 'BTC-USD-SWAP',
        last: '40000',
        open24h: '40000',
        bidPx: '39999',
        askPx: '40001',
        volCcy24h: '1',
        vol24h: '1',
      },
    ]);
    for (const socket of [exchange.publicWebsocket, exchange.businessWebsocket, exchange.privateWebsocket]) {
      vi.spyOn(socket, 'connectAndSubscribe').mockImplementation(() => {});
    }
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue(store.orders);
    vi.spyOn(exchange, 'fetchBalanceAndPositions').mockImplementation(() => {
      const completed = Promise.resolve({ balance: stale, positions: store.positions });
      // The read has completed; a buffered account frame precedes its .then commit.
      queueMicrotask(() => {
        beforeWs = writes.mock.calls.length;
        exchange.applyAccountEquity([{ totalEq: '40400', details: [{ ccy: 'USD', eq: '400' }] }]);
      });
      return completed;
    });
    await exchange.start();
    expect(beforeWs).toBeGreaterThanOrEqual(0);
    const totals = writes.mock.calls
      .slice(beforeWs)
      .flatMap(([update]) => (update.balance ? [update.balance.total] : []));
    expect(totals.length).toBeGreaterThanOrEqual(2);
    expect(totals.every((total) => total === 40387.18)).toBe(true);
    expect(store.balance).toMatchObject({ total: 40387.18, free: 40317 });
    expect(exchange.xhr.get).not.toHaveBeenCalled();
  });

  it('retains WS equity across an older REST join, then accepts a fresh REST snapshot', async () => {
    const { exchange, store } = fixture();
    let resolveRisk!: (value: any) => void;
    const risk = new Promise<any>((resolve) => {
      resolveRisk = resolve;
    });
    const get = vi.mocked(exchange.xhr.get);
    get.mockImplementation((url) => {
      if (String(url).includes('account-position-risk')) return risk;
      if (String(url).endsWith('/positions')) return Promise.resolve({ data: { data: [] } });
      throw new Error(`Unexpected request ${url}`);
    });
    vi.spyOn(exchange, 'mapPositions').mockReturnValue(store.positions);
    const read = exchange.fetchBalanceAndOpenPositions();
    exchange.applyAccountEquity([{ totalEq: '40400', details: [{ ccy: 'USD', eq: '400' }] }]);
    resolveRisk({
      data: {
        data: [
          {
            balData: [
              { ccy: 'BTC', eq: '1' },
              { ccy: 'USD', eq: '100' },
            ],
          },
        ],
      },
    });
    const result = await read;
    expect(result.balance).toMatchObject({ total: 40387.18, free: 40317 });
    get.mockResolvedValueOnce({
      data: {
        data: [
          {
            balData: [
              { ccy: 'BTC', eq: '1' },
              { ccy: 'USD', eq: '200' },
            ],
          },
        ],
      },
    });
    const fresh = await exchange.fetchBalanceAndOpenPositions();
    expect(fresh.balance).toMatchObject({ total: 40187.18, free: 40117 });
  });
  it('ignores the entire malformed update without poisoning a later REST snapshot fence', async () => {
    const { exchange, store } = fixture();
    const before = store.balance;
    for (const data of [
      [],
      [{ totalEq: 'NaN' }],
      [{ totalEq: '40400', details: [{ ccy: 'USD', eq: 'NaN' }] }],
      [{ totalEq: '40400', details: {} }],
    ]) {
      exchange.applyAccountEquity(data);
      expect(store.balance).toBe(before);
    }
    expect(await exchange.formatBalanceUsedAvail(before.info, store.positions, store.orders)).toMatchObject({
      total: 40087.18,
      free: 40017,
    });
  });
  it('accepts a scalar-only account push without inventing a USD currency row', () => {
    const { exchange, store } = fixture();
    exchange.applyAccountEquity([{ totalEq: '40400' }]);
    expect(store.balance.total).toBe(40387.18);
    expect(store.balance.currencies?.USD.total).toBe(100);
    expect(store.balance.currencies?.BTC.total).toBeCloseTo(0.9996794871794872);
  });
  it('validates all account rows and details atomically', () => {
    expect(parseAccountEquity([{ totalEq: '1' }, { totalEq: 'bad' }])).toBeNull();
    expect(parseAccountEquity([{ totalEq: '0x10' }])).toBeNull();
    expect(
      parseAccountEquity([
        {
          totalEq: '1',
          details: [
            { ccy: 'USD', eq: '1' },
            { ccy: 'USD', eq: '2' },
          ],
        },
      ]),
    ).toBeNull();
    expect(parseAccountEquity([{ totalEq: '1', details: [{ ccy: '', eq: '1' }] }])).toBeNull();
    expect(parseAccountEquity([{ totalEq: '1', details: [{ ccy: 'USD', eq: '1' }] }])).toEqual({
      total: 1,
      details: [{ ccy: 'USD', eq: '1' }],
    });
  });
});
