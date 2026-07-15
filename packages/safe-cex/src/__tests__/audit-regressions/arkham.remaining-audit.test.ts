import { afterEach, describe, expect, it, vi } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { ArkhamExchange } from '../../exchanges/arkham/arkham.exchange';
import { DefaultStore } from '../../store/store.base';
import { LogSeverity, OrderSide, OrderStatus, OrderType, PositionSide } from '../../types';
import { createPositionId } from '../../utils/createPositionId';

const createExchange = (options: Record<string, unknown> = {}) => {
  const store = new DefaultStore();
  const exchange = new ArkhamExchange(
    'arkham',
    { key: 'key', secret: Buffer.from('secret').toString('base64'), ...options },
    store,
  );
  return { exchange, store };
};

const position = (symbol: string, side = PositionSide.Long, contracts = 1) => ({
  id: createPositionId(symbol, side),
  symbol,
  side,
  contracts,
  notional: contracts * 50_000,
  unrealizedPnl: 0,
  leverage: 5,
  entryPrice: 50_000,
  liquidationPrice: 0,
  info: { initialMargin: String(contracts * 10_000) },
});

const order = (id: string, symbol = 'BTC_USDT_PERP') => ({
  id,
  symbol,
  side: OrderSide.Buy,
  type: OrderType.Limit,
  status: OrderStatus.Open,
  price: 50_000,
  amount: 1,
  filled: 0,
  remaining: 1,
  reduceOnly: false,
  lastUpdate: 1,
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('Arkham remaining audit regressions', () => {
  it('uses a stable REST ticker id and updates the stored ticker object', async () => {
    const { exchange, store } = createExchange();
    const restTicker = {
      symbol: 'BTC_USDT_PERP',
      price: '50000',
      price24hAgo: '49000',
      markPrice: '50001',
      indexPrice: '49999',
      fundingRate: '0.0001',
      nextFundingTime: 123,
      volume24h: '12',
      quoteVolume24h: '600000',
      openInterest: '40',
    };
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({ data: [restTicker] }),
    } as any;

    const first = await exchange.fetchTickers();
    const second = await exchange.fetchTickers();

    expect(first[0].id).toBe(restTicker.symbol);
    expect(second[0].id).toBe(restTicker.symbol);

    store.update({ tickers: first });
    exchange.publicWebsocket.messageHandlers.ticker[0]({
      symbol: restTicker.symbol,
      bid_price: '50010',
      ask_price: '50020',
      last_price: '50015',
      price_change_percent_24h: '2.5',
      volume_24h: '14',
      quote_volume_24h: '700000',
      funding_rate: '0.0002',
      next_funding_time: '456',
      open_interest: '44',
    });

    expect(store.tickers).toHaveLength(1);
    expect(store.tickers[0]).toMatchObject({
      id: restTicker.symbol,
      symbol: restTicker.symbol,
      bid: 50_010,
      ask: 50_020,
      last: 50_015,
      mark: 50_015,
      index: 50_015,
    });
    exchange.dispose();
  });

  it('applies every cheap private order and position event without global throttling', () => {
    const { exchange, store } = createExchange();
    const websocket = exchange.privateWebsocket as any;
    vi.spyOn(websocket, 'throttledFetchOrders').mockImplementation(() => undefined);
    vi.spyOn(websocket, 'throttledFetchPositions').mockImplementation(() => undefined);

    websocket.handleOrderUpdate({
      orderId: 1,
      symbol: 'BTC_USDT_PERP',
      side: 'buy',
      type: 'limitGtc',
      status: 'booked',
      price: '50000',
      size: '1',
      executedSize: '0',
    });
    websocket.handleOrderUpdate({
      orderId: 2,
      symbol: 'ETH_USDT_PERP',
      side: 'sell',
      type: 'limitGtc',
      status: 'booked',
      price: '3000',
      size: '2',
      executedSize: '0',
    });
    websocket.handlePositionUpdate({
      symbol: 'BTC_USDT_PERP',
      base: '1',
      quote: '50000',
      value: '50000',
      pnl: '10',
      initialMargin: '10000',
      averageEntryPrice: '49990',
    });
    websocket.handlePositionUpdate({
      symbol: 'ETH_USDT_PERP',
      base: '-2',
      quote: '-6000',
      value: '6000',
      pnl: '-4',
      initialMargin: '1200',
      averageEntryPrice: '2998',
    });

    expect(store.orders.map(({ id }) => id)).toEqual(['1', '2']);
    expect(store.positions.map(({ symbol, side }) => [symbol, side])).toEqual([
      ['BTC_USDT_PERP', PositionSide.Long],
      ['ETH_USDT_PERP', PositionSide.Short],
    ]);
    exchange.dispose();
  });

  it('upserts a zero-contract position and clears the released used margin', () => {
    const { exchange, store } = createExchange();
    const websocket = exchange.privateWebsocket as any;
    vi.spyOn(websocket, 'throttledFetchPositions').mockImplementation(() => undefined);
    store.update({
      positions: [position('BTC_USDT_PERP', PositionSide.Short, 2)],
      balance: { total: 100_000, free: 80_000, used: 20_000, upnl: 0 },
    });

    websocket.handlePositionUpdate({
      symbol: 'BTC_USDT_PERP',
      base: '0',
      quote: '0',
      value: '0',
      pnl: '0',
      initialMargin: '0',
      averageEntryPrice: '0',
    });

    expect(store.positions).toEqual([
      expect.objectContaining({
        id: createPositionId('BTC_USDT_PERP', PositionSide.Short),
        side: PositionSide.Short,
        contracts: 0,
        notional: 0,
      }),
    ]);
    expect(store.balance).toMatchObject({ total: 100_000, used: 0, free: 100_000 });
    exchange.dispose();
  });

  it('commits throttled authoritative REST results as full snapshots', async () => {
    const { exchange, store } = createExchange();
    const websocket = exchange.privateWebsocket as any;
    store.update({
      orders: [order('stale')],
      positions: [position('BTC_USDT_PERP')],
    });
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([order('fresh', 'ETH_USDT_PERP')]);
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);

    await websocket.executeOrderFetch();
    await websocket.executePositionFetch();

    expect(store.orders).toEqual([expect.objectContaining({ id: 'fresh', symbol: 'ETH_USDT_PERP' })]);
    expect(store.positions).toEqual([
      expect.objectContaining({ id: createPositionId('BTC_USDT_PERP', PositionSide.Long), contracts: 0 }),
    ]);
    expect(store.loaded.orders).toBe(true);
    expect(store.loaded.positions).toBe(true);
    exchange.dispose();
  });

  it('fails closed before opening the legacy public proxy without credentials', () => {
    const { exchange } = createExchange({ key: '', secret: '', publicDataOnly: true });
    const createWebsocket = vi.spyOn(exchange.publicWebsocket as any, 'createWebsocket');
    const log = vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    exchange.publicWebsocket.connectAndSubscribe();

    expect(createWebsocket).not.toHaveBeenCalled();
    expect(log).toHaveBeenCalledWith(
      'Arkham public WebSocket requires a legacy API key and secret; skipping the connection',
      LogSeverity.Warning,
    );
    exchange.dispose();
  });

  it.each(['publicWebsocket', 'privateWebsocket'] as const)(
    'keeps one heartbeat timer and clears it on close and disposal for %s',
    (socketName) => {
      vi.useFakeTimers();
      const { exchange } = createExchange();
      const websocket = exchange[socketName] as any;
      websocket.ws = { readyState: 1, send: vi.fn(), close: vi.fn() };

      websocket.ping();
      websocket.ping();
      expect(vi.getTimerCount()).toBe(1);

      websocket.handleClose();
      expect(vi.getTimerCount()).toBe(0);

      websocket.ping();
      expect(vi.getTimerCount()).toBe(1);
      websocket.dispose();
      expect(vi.getTimerCount()).toBe(0);
      exchange.dispose();
    },
  );

  it('keeps Arkham disabled while advertising shared reduce-order downsizing', () => {
    expect(exchangeMetadata.arkham).toMatchObject({
      enabled: false,
      enabledInDev: false,
      enableInPreview: false,
      autoDownSizeReduceOrders: true,
    });
  });
});
