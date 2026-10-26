import { describe, expect, it, vi } from 'vitest';

import { BinancePublicWebsocket } from '../reference/phemex/phemex.ws-public';
import { DefaultStore } from '../../store/store.base';

const createWebsocket = () => {
  const parent = {
    emitter: { emit: vi.fn() },
    formatSymbolIn: (symbol: string) => symbol,
    isDisposed: false,
    options: { testnet: false },
    store: {},
    timeframes: { '1m': '60', '5m': '300' },
  } as any;

  return new BinancePublicWebsocket(parent);
};

const createWebsocketWithStore = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'ADA/USDT:USDT',
        symbol: 'ADAUSDT',
        base: 'ADA',
        quote: 'USDT',
        active: true,
        info: { symbol: 'ADAUSDT' },
      },
    ] as any,
  });

  const parent = {
    emitter: { emit: vi.fn() },
    formatSymbolIn: (symbol: string) => symbol,
    isDisposed: false,
    options: { testnet: false },
    store,
    timeframes: { '1m': '60', '5m': '300' },
  } as any;

  return { store, websocket: new BinancePublicWebsocket(parent) };
};

describe('Phemex public WebSocket dispatch', () => {
  it('routes the packed ticker by its exact method and preserves per-callback stats', () => {
    const websocket = createWebsocket();
    const first = vi.fn();
    const second = vi.fn();
    websocket.messageHandlers = {
      'perp_market24h_pack_p.update': [first, second],
    };
    const recordStats = vi.spyOn(websocket as any, 'recordMessageHandlerStats');

    websocket.onMessage({
      data: JSON.stringify({ note: 'decoy perp_market24h_pack_p.update text' }),
    } as MessageEvent);

    expect(first).not.toHaveBeenCalled();
    expect(second).not.toHaveBeenCalled();
    expect(recordStats).not.toHaveBeenCalled();

    const ticker = {
      data: [['BTCUSDT', '60000', '61000']],
      method: 'perp_market24h_pack_p.update',
      type: 'snapshot',
    };
    websocket.onMessage({ data: JSON.stringify(ticker) } as MessageEvent);

    expect(first).toHaveBeenCalledWith([ticker]);
    expect(second).toHaveBeenCalledWith([ticker]);
    expect(recordStats).toHaveBeenCalledTimes(2);
    expect(recordStats).toHaveBeenNthCalledWith(1, 'perp_market24h_pack_p.update');
    expect(recordStats).toHaveBeenNthCalledWith(2, 'perp_market24h_pack_p.update');
  });

  it('extracts the kline interval and symbol and dispatches only the exact handler', () => {
    const websocket = createWebsocket();
    const btcOneMinute = vi.fn();
    const btcFiveMinute = vi.fn();
    const ethOneMinute = vi.fn();
    websocket.messageHandlers = {
      'kline_p.1m.BTCUSDT': [btcOneMinute],
      'kline_p.5m.BTCUSDT': [btcFiveMinute],
      'kline_p.1m.ETHUSDT': [ethOneMinute],
    };
    const recordStats = vi.spyOn(websocket as any, 'recordMessageHandlerStats');
    const rows = [
      [1_700_000_060, 60, '60000', '60001', '60002', '59999', '60001', '2', '120002'],
      [1_700_000_000, 60, '59990', '60000', '60001', '59989', '60000', '3', '180000'],
    ];

    websocket.onMessage({
      data: JSON.stringify({ kline_p: rows, sequence: 123, symbol: 'BTCUSDT', type: 'snapshot' }),
    } as MessageEvent);

    expect(btcOneMinute).toHaveBeenCalledOnce();
    expect(btcOneMinute).toHaveBeenCalledWith(rows);
    expect(btcFiveMinute).not.toHaveBeenCalled();
    expect(ethOneMinute).not.toHaveBeenCalled();
    expect(recordStats).toHaveBeenCalledOnce();
    expect(recordStats).toHaveBeenCalledWith('kline_p.1m.BTCUSDT');
  });

  it('routes orderbook frames by exact symbol and records one topic hit', () => {
    const websocket = createWebsocket();
    const btc = vi.fn();
    const eth = vi.fn();
    websocket.messageHandlers = {
      'orderbook_p.BTCUSDT': [btc],
      'orderbook_p.ETHUSDT': [eth],
    };
    const recordStats = vi.spyOn(websocket as any, 'recordMessageHandlerStats');
    const orderbook = { asks: [['60001', '1']], bids: [['60000', '2']] };

    websocket.onMessage({
      data: JSON.stringify({ orderbook_p: orderbook, sequence: 456, symbol: 'BTCUSDT', type: 'incremental' }),
    } as MessageEvent);

    expect(btc).toHaveBeenCalledOnce();
    expect(btc).toHaveBeenCalledWith({ ...orderbook, sequence: 456, type: 'incremental', depth: undefined });
    expect(eth).not.toHaveBeenCalled();
    expect(recordStats).toHaveBeenCalledOnce();
    expect(recordStats).toHaveBeenCalledWith('orderbook_p.BTCUSDT');
  });

  it('seeds a ticker from packed updates when the REST baseline is empty', () => {
    const { store, websocket } = createWebsocketWithStore();

    websocket.handleTickerStreamEvents([
      {
        data: [
          [
            'ADAUSDT',
            '0.18',
            undefined,
            undefined,
            '0.189',
            '1000',
            undefined,
            '2000',
            '0.1888',
            '0.1891',
            '0.0001',
            undefined,
            '0.1889',
            '0.1892',
          ],
        ],
      },
    ]);

    expect(store.tickersBySymbol.ADAUSDT).toEqual({
      id: 'ADA/USDT:USDT',
      symbol: 'ADAUSDT',
      bid: 0.1889,
      ask: 0.1892,
      last: 0.189,
      mark: 0.1891,
      index: 0.1888,
      percentage: 5.000000000000004,
      openInterest: 2000,
      fundingRate: 0.0001,
      fundingTime: 0,
      volume: 1000,
      quoteVolume: 189,
    });
  });
});
