import { describe, expect, it, vi } from 'vitest';

import { BinancePublicWebsocket } from '../../exchanges/phemex/phemex.ws-public';

const createWebsocket = () => {
  const parent = {
    emitter: { emit: vi.fn() },
    isDisposed: false,
    options: { testnet: false },
    store: {},
    timeframes: { '1m': '60', '5m': '300' },
  } as any;

  return new BinancePublicWebsocket(parent);
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
    expect(btc).toHaveBeenCalledWith(orderbook);
    expect(eth).not.toHaveBeenCalled();
    expect(recordStats).toHaveBeenCalledOnce();
    expect(recordStats).toHaveBeenCalledWith('orderbook_p.BTCUSDT');
  });
});
