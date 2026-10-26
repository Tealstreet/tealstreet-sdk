import type { ExchangeOptions, Market, Timeframe } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { OKXExchange } from '../reference/okx/okx.exchange';
import { ENDPOINTS, INTERVAL } from '../../exchanges/okx/okx.types';
import { DefaultStore } from '../../store/store.base';

// OKX v5 candles: `bar=1M` is a Hong Kong-time month and `1Mutc` the UTC
// calendar month (https://www.okx.com/docs-v5/en/#public-data-rest-api-get-candlesticks;
// WS channel `candle1Mutc`). Rows captured from www.okx.com on 2026-10-10:
// `bar=1Mutc` opened 2026-10-01T00:00Z, while `bar=1M` opened 2026-09-30T16:00Z.
const MONTH = '1M' as Timeframe;
const utc = (year: number, month: number, day: number, hour = 0) => Date.UTC(year, month - 1, day, hour);

const LIVE_1MUTC_ROWS = [
  ['1790812800000', '83579.8', '87239', '80351', '82735', '65104012.1', '651040.121', '54851685452.35689', '0'],
  [
    '1788220800000',
    '78549.9',
    '87374.3',
    '74896.6',
    '83579.9',
    '222346834.29',
    '2223468.3429',
    '178915511256.87855',
    '1',
  ],
  ['1785542400000', '62858', '81520', '62227', '78549.9', '233662604.3', '2336626.043', '165855392561.8832', '1'],
];

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
  precision: { amount: 1, price: 0.1 },
  limits: {
    amount: { min: 1, max: 100 },
    leverage: { min: 1, max: 100 },
  },
  contractSize: 0.01,
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [market], loaded: { ...store.loaded, markets: true } });
  return new OKXExchange('okx', options, store);
};

let exchange: OKXExchange | undefined;

afterEach(() => {
  exchange?.dispose();
  exchange = undefined;
  vi.restoreAllMocks();
});

describe('OKX monthly (1M) candles', () => {
  it('maps the chart 1M interval to the UTC month, leaving 1D and 1W unchanged', () => {
    expect(INTERVAL[MONTH as '1M']).toBe('1Mutc');
    expect(INTERVAL['1d']).toBe('1D');
    expect(INTERVAL['1w']).toBe('1W');
  });

  it('requests bar=1Mutc and returns 1st-of-month UTC candles', async () => {
    exchange = createExchange();
    const get = vi.spyOn(exchange.xhr, 'get').mockResolvedValueOnce({ data: { data: LIVE_1MUTC_ROWS } } as never);

    const candles = await exchange.fetchOHLCV({
      symbol: market.symbol,
      interval: MONTH,
      to: utc(2026, 10, 1),
      limit: 3,
    });

    expect(get).toHaveBeenCalledOnce();
    expect(get.mock.calls[0][0]).toBe(ENDPOINTS.KLINE);
    expect((get.mock.calls[0][1] as { params: Record<string, unknown> }).params).toMatchObject({
      instId: market.id,
      bar: '1Mutc',
      after: utc(2026, 10, 1),
    });
    expect(candles.map((candle) => candle.timestamp * 1000)).toEqual([
      utc(2026, 8, 1),
      utc(2026, 9, 1),
      utc(2026, 10, 1),
    ]);
    expect(candles[2]).toMatchObject({ open: 83579.8, close: 82735, volume: 54851685452.35689 });
  });

  it('subscribes candle1Mutc and routes its frames to the 1M handler', () => {
    exchange = createExchange();
    const websocket = exchange.businessWebsocket;
    const send = vi.spyOn(websocket, 'send').mockImplementation(() => {});
    vi.spyOn(websocket, 'isConnected', 'get').mockReturnValue(true);
    const callback = vi.fn();

    websocket.listenOHLCV({ symbol: market.symbol, interval: MONTH }, callback);

    expect(JSON.parse(send.mock.calls[0][0] as string)).toEqual({
      op: 'subscribe',
      args: [{ channel: 'candle1Mutc', instId: market.id }],
    });

    websocket.onMessage({
      data: JSON.stringify({ arg: { channel: 'candle1Mutc', instId: market.id }, data: [LIVE_1MUTC_ROWS[0]] }),
    } as MessageEvent);

    expect(callback).toHaveBeenCalledWith(
      expect.objectContaining({ timestamp: utc(2026, 10, 1) / 1000, open: 83579.8, close: 82735 }),
    );
    expect(Object.keys(websocket.messageHandlersStats)).toEqual([`candle.1M.${market.id}`]);
  });
});
