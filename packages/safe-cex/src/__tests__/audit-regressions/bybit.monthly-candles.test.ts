import type { Candle, Timeframe } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { INTERVAL } from '../../exchanges/bybit/bybit.types';
import { BybitPublicWebsocket } from '../reference/bybit/bybit.ws-public';
import { BybitInverseExchange } from '../reference/bybit/bybitinverse.exchange';
import { BybitInversePublicWebsocket } from '../reference/bybit/bybitinverse.ws-public';
import { BybitSpotExchange } from '../reference/bybit/bybitspot.exchange';
import { BybitPublicWebsocket as BybitSpotPublicWebsocket } from '../reference/bybit/bybitspot.ws-public';

// Bybit v5 kline: `interval=M` is the UTC calendar month, `W` the Monday week
// (https://bybit-exchange.github.io/docs/v5/market/kline). The REST rows and
// WS frames below were captured from api.bybit.com / stream.bybit.com on
// 2026-10-10 (`category=linear&symbol=BTCUSDT&interval=M`, `kline.M.BTCUSDT`).
const MONTH = '1M' as Timeframe;
const utc = (year: number, month: number, day: number, hour = 0) => Date.UTC(year, month - 1, day, hour);

const LIVE_MONTH_ROWS = [
  ['1790812800000', '83573.8', '87242.2', '80350.1', '82736.6', '505048.582', '42550985870.0303'],
  ['1788220800000', '78541.8', '87383.6', '74919.6', '83573.8', '1712214.913', '137889762829.2341'],
  ['1785542400000', '62857.8', '81499.9', '62240', '78541.8', '1835101.482', '129877727699.1292'],
];

const createStore = () =>
  ({
    subscribe: vi.fn(),
    markets: [],
    marketsBySymbol: {},
    tickersBySymbol: {},
    orders: [],
    positions: [],
  }) as any;

const options = { key: 'key', secret: 'secret', testnet: false } as any;

describe('Bybit monthly (1M) candles', () => {
  it('maps the chart 1M interval to Bybit M, keeping W for weeks', () => {
    expect(INTERVAL[MONTH as '1M']).toBe('M');
    expect(INTERVAL['1w']).toBe('W');
  });

  it.each([
    ['linear', () => new BybitExchange('bybit' as any, options, createStore()), 'linear'],
    ['inverse', () => new BybitInverseExchange('bybitinverse' as any, options, createStore()), 'inverse'],
    ['spot', () => new BybitSpotExchange('bybitspot' as any, options, createStore()), 'spot'],
  ])('%s: requests interval=M and returns the 1st-of-month UTC candles', async (_name, create, category) => {
    const exchange = create();
    const get = vi.fn().mockResolvedValue({ data: { retCode: 0, result: { list: LIVE_MONTH_ROWS } } });
    exchange.xhr = { get } as any;

    const candles: Candle[] = await exchange.fetchOHLCV({
      symbol: category === 'inverse' ? 'BTCUSD' : 'BTCUSDT',
      interval: MONTH,
      to: utc(2026, 10, 1),
      limit: 3,
    });

    const params = get.mock.calls[0][1].params;
    expect(params).toMatchObject({ category, interval: 'M', limit: 3, end: utc(2026, 10, 1) });
    // `limit` months back from `end` (dayjs month arithmetic, local DST aside).
    expect(Math.abs(params.start - utc(2026, 7, 1))).toBeLessThanOrEqual(3_600_000);

    expect(candles.map((candle) => candle.timestamp * 1000)).toEqual([
      utc(2026, 8, 1),
      utc(2026, 9, 1),
      utc(2026, 10, 1),
    ]);
    expect(candles[2]).toMatchObject({ open: 83573.8, high: 87242.2, low: 80350.1, close: 82736.6 });
  });
});

const makeParent = (name: string) =>
  ({
    isDisposed: false,
    isDemo: false,
    name,
    options: { testnet: false },
    emitter: { emit: vi.fn(), off: vi.fn(), on: vi.fn() },
    on: vi.fn(),
    off: vi.fn(),
    store: { markets: [], update: vi.fn() },
    log: vi.fn(),
    syncSocketHealthToStore: vi.fn(),
    emitWsReconnected: vi.fn(),
    getTickerSymbolsForPublicStream: () => [],
    // Mirrors `BybitExchange.publicKlineSubscribeChunkSize`; `listenOHLCVs`
    // reads this to size bulk kline subscribe frames. Must stay 50 for the
    // Bybit flavours and must not shrink to the Hyro override (10) here.
    publicKlineSubscribeChunkSize: 50,
  }) as any;

const openSocket = <T extends { ws: WebSocket | undefined }>(websocket: T) => {
  const send = vi.fn();
  websocket.ws = { readyState: WebSocket.OPEN, send } as unknown as WebSocket;
  return send;
};

const sockets = [
  ['linear', () => new BybitPublicWebsocket(makeParent('bybit'))],
  ['inverse', () => new BybitInversePublicWebsocket(makeParent('bybitinverse'))],
  ['spot', () => new BybitSpotPublicWebsocket(makeParent('bybitspot'))],
] as const;

const klineFrame = (startMs: number, close = '100') => ({
  data: [{ start: startMs, open: '100', high: '100', low: '100', close, volume: '1' }],
});
const tradeFrame = (timeMs: number, price = '101') => ({ data: [{ T: timeMs, p: price, v: '1', S: 'Buy' }] });

describe('Bybit 1w / 1M live bar', () => {
  it.each(sockets)('%s: subscribes kline.M and keeps trades inside the calendar month', (_name, create) => {
    vi.useFakeTimers();
    try {
      const websocket = create() as any;
      const send = openSocket(websocket);
      const callback = vi.fn();

      websocket.listenOHLCV({ symbol: 'BTCUSDT', interval: MONTH }, callback);
      const subscribed = send.mock.calls.map(([payload]) => JSON.parse(payload as string).args[0]);
      expect(subscribed).toContain('kline.M.BTCUSDT');

      websocket.messageHandlers['kline.M.BTCUSDT'][0](klineFrame(utc(2026, 10, 1)));
      // The last minute of a 31-day month is still that month's bar.
      websocket.messageHandlers['publicTrade.BTCUSDT'][0](tradeFrame(utc(2026, 10, 31, 23) + 59 * 60_000));
      expect(callback.mock.lastCall?.[0]).toMatchObject({ timestamp: utc(2026, 10, 1) / 1000, close: 101 });

      // The first trade of November opens November's bar at October's close.
      websocket.messageHandlers['publicTrade.BTCUSDT'][0](tradeFrame(utc(2026, 11, 1), '102'));
      expect(callback.mock.lastCall?.[0]).toMatchObject({ timestamp: utc(2026, 11, 1) / 1000, open: 101, close: 102 });
    } finally {
      vi.useRealTimers();
    }
  });

  it.each(sockets)(
    '%s: a Saturday trade extends the Monday week instead of opening a Thursday bar',
    (_name, create) => {
      vi.useFakeTimers();
      try {
        const websocket = create() as any;
        openSocket(websocket);
        const callback = vi.fn();
        const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

        websocket.listenOHLCV({ symbol: 'BTCUSDT', interval: '1w' }, callback);
        websocket.messageHandlers['kline.W.BTCUSDT'][0](klineFrame(utc(2026, 10, 5)));
        // Saturday: the epoch floor put this on Thursday 2026-10-08.
        websocket.messageHandlers['publicTrade.BTCUSDT'][0](tradeFrame(utc(2026, 10, 10, 12)));
        expect(callback.mock.lastCall?.[0]).toMatchObject({ timestamp: utc(2026, 10, 5) / 1000, close: 101 });

        // A later kline frame for the same Monday is still accepted.
        websocket.messageHandlers['kline.W.BTCUSDT'][0](klineFrame(utc(2026, 10, 5), '103'));
        expect(callback.mock.lastCall?.[0]).toMatchObject({ timestamp: utc(2026, 10, 5) / 1000, close: 103 });
        expect(warn).not.toHaveBeenCalled();
        warn.mockRestore();
      } finally {
        vi.useRealTimers();
      }
    },
  );

  it('linear: maps kline.M frames on the multi-symbol stream back to 1M', () => {
    const websocket = new BybitPublicWebsocket(makeParent('bybit'));
    const send = openSocket(websocket as any);
    const callback = vi.fn();

    websocket.listenOHLCVs({ symbols: ['BTCUSDT', 'ETHUSDT'], interval: MONTH }, callback);
    expect(JSON.parse(send.mock.calls[0][0]).args).toEqual(['kline.M.BTCUSDT', 'kline.M.ETHUSDT']);

    websocket.messageHandlers.kline[0]({ topic: 'kline.M.ETHUSDT', ...klineFrame(utc(2026, 10, 1)) });
    expect(callback).toHaveBeenCalledWith(
      expect.objectContaining({ symbol: 'ETHUSDT', interval: '1M', timestamp: utc(2026, 10, 1) / 1000 }),
    );
  });
});
