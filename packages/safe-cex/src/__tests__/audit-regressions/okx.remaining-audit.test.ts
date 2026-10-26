import type { ExchangeOptions, Market } from '../../types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { OKXExchange } from '../reference/okx/okx.exchange';
import { ENDPOINTS } from '../../exchanges/okx/okx.types';
import { DefaultStore } from '../../store/store.base';

const options: ExchangeOptions = {
  key: 'test-key',
  secret: 'test-secret',
  passphrase: 'test-passphrase',
  testnet: false,
  tuning: { consoleLogging: false },
};

const market: Market = {
  id: 'BTC-USDT-SWAP',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 5, price: 0.1 },
  limits: {
    amount: { min: 1, max: 100 },
    leverage: { min: 1, max: 100 },
  },
  contractSize: 0.01,
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [market],
    tickers: [
      {
        id: market.id,
        symbol: market.symbol,
        bid: 99,
        ask: 101,
        last: 100,
        mark: 100,
        index: 100,
        percentage: 0,
        fundingRate: 0,
        fundingTime: 0,
        volume: 0,
        quoteVolume: 0,
        openInterest: 0,
      },
    ],
    loaded: { ...store.loaded, markets: true, tickers: true },
  });
  return new OKXExchange('okx', options, store);
};

describe('OKX remaining July audit regressions', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('emits private websocket fill amounts in base units using contract size', () => {
    const exchange = createExchange();
    const emit = vi.spyOn(exchange.emitter, 'emit');

    exchange.privateWebsocket.handleOrderTopic({
      data: [
        {
          instId: market.id,
          ordId: 'order-1',
          tradeId: 'trade-1',
          state: 'partially_filled',
          ordType: 'limit',
          side: 'buy',
          sz: '10',
          accFillSz: '2',
          fillSz: '2',
          fillPx: '100',
          px: '100',
          reduceOnly: 'false',
        },
      ],
    });

    expect(emit).toHaveBeenCalledWith(
      'fill',
      expect.objectContaining({ id: 'order-1', symbol: market.symbol, price: 100, amount: 0.02 }),
    );
  });

  it('preserves regular order cumulative fill in placement units', () => {
    const exchange = createExchange();

    const [order] = exchange.mapOrders([
      {
        instId: market.id,
        ordId: 'order-1',
        state: 'partially_filled',
        ordType: 'limit',
        side: 'buy',
        sz: '10',
        accFillSz: '2',
        px: '100',
        avgPx: '100',
        reduceOnly: 'false',
      },
    ]);

    expect(order.amount).toBe(10);
    expect(order.filled).toBe(0.02);
    expect(order.remaining).toBe(0.08);
    expect(order.extra?.remainingNative).toBe(8);
    expect(order.extra?.filledInPlacementUnits).toBe(2);
  });

  it('resubscribes for a fresh orderbook snapshot when sequence continuity breaks', () => {
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    websocket.ws = { readyState: WebSocket.OPEN } as WebSocket;
    const send = vi.spyOn(websocket, 'send').mockImplementation(() => {});
    const callback = vi.fn();
    websocket.listenOrderBook(market.symbol, callback);

    const push = (action: 'snapshot' | 'update', seqId: number, prevSeqId: number, bid = '99') => {
      websocket.onMessage({
        data: JSON.stringify({
          arg: { channel: 'books', instId: market.id },
          action,
          data: [{ bids: [[bid, '1']], asks: [['101', '1']], seqId, prevSeqId }],
        }),
      } as MessageEvent);
    };

    push('snapshot', 100, -1);
    push('update', 101, 100, '98');
    push('update', 103, 999, '97');
    push('update', 104, 103, '96');

    expect(callback).toHaveBeenCalledTimes(2);
    expect(send.mock.calls.map(([payload]) => payload)).toEqual([
      JSON.stringify({ op: 'subscribe', args: [{ channel: 'books', instId: market.id }] }),
      JSON.stringify({ op: 'unsubscribe', args: [{ channel: 'books', instId: market.id }] }),
      JSON.stringify({ op: 'subscribe', args: [{ channel: 'books', instId: market.id }] }),
    ]);

    push('snapshot', 200, -1);
    push('update', 201, 200, '95');
    expect(callback).toHaveBeenCalledTimes(4);
  });

  it('uses history-candles when the recent endpoint is empty', async () => {
    const exchange = createExchange();
    const get = vi
      .spyOn(exchange.xhr, 'get')
      .mockResolvedValueOnce({ data: { data: [] } } as never)
      .mockResolvedValueOnce({
        data: { data: [['1700000000000', '1', '2', '0.5', '1.5', '0', '0', '12']] },
      } as never);

    await expect(exchange.fetchOHLCV({ symbol: market.symbol, interval: '1m', limit: 300 })).resolves.toEqual([
      { timestamp: 1_700_000_000, open: 1, high: 2, low: 0.5, close: 1.5, volume: 12 },
    ]);

    expect(get.mock.calls.map(([endpoint]) => endpoint)).toEqual([ENDPOINTS.KLINE, ENDPOINTS.KLINE_HISTORY]);
    const historyRequest = get.mock.calls[1][1] as { params: { limit: number } };
    expect(historyRequest.params.limit).toBe(100);
  });

  it('uses history-candles for a requested window older than the recent retention window', async () => {
    const exchange = createExchange();
    const get = vi
      .spyOn(exchange.xhr, 'get')
      .mockResolvedValueOnce({
        data: { data: [['1700000060000', '9', '9', '9', '9', '0', '0', '9']] },
      } as never)
      .mockResolvedValueOnce({
        data: { data: [['1700000000000', '1', '2', '0.5', '1.5', '0', '0', '12']] },
      } as never);

    const candles = await exchange.fetchOHLCV({
      symbol: market.symbol,
      interval: '1m',
      limit: 100,
      to: 1_700_000_060_000,
    });

    expect(candles[0]?.open).toBe(1);
    expect(get.mock.calls.map(([endpoint]) => endpoint)).toEqual([ENDPOINTS.KLINE, ENDPOINTS.KLINE_HISTORY]);
  });
});
