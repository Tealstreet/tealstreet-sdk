import type { ExchangeOptions, Market } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { OKXExchange } from '../reference/okx/okx.exchange';
import { ORDER_STATS_TOPIC } from '../reference/okx/okx.ws-business';
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
  precision: { amount: 1, price: 0.1 },
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
    loaded: { ...store.loaded, markets: true },
  });
  return new OKXExchange('okx', options, store);
};

let exchange: OKXExchange | undefined;

afterEach(() => {
  exchange?.dispose();
  exchange = undefined;
  vi.restoreAllMocks();
});

describe('OKX websocket exact-topic routing', () => {
  it('routes a public frame from arg only and records stats only for matched handlers', () => {
    exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    const ticker = vi.fn();
    const wrongBook = vi.fn();

    websocket.messageHandlers = {
      tickers: [ticker],
      'books.ETH-USDT-SWAP': [wrongBook],
    };

    websocket.onMessage({
      data: JSON.stringify({
        arg: { channel: 'tickers', instId: market.id },
        data: [{ channel: 'books', instId: 'ETH-USDT-SWAP', last: '100' }],
      }),
    } as MessageEvent);

    expect(ticker).toHaveBeenCalledOnce();
    expect(wrongBook).not.toHaveBeenCalled();
    expect(Object.keys(websocket.messageHandlersStats)).toEqual(['tickers']);
  });

  it('routes public orderbooks by the exact channel and instrument pair', () => {
    exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    const matchingBook = vi.fn();
    const wrongBook = vi.fn();

    websocket.messageHandlers = {
      [`books.${market.id}`]: [matchingBook],
      'books.ETH-USDT-SWAP': [wrongBook],
    };

    websocket.onMessage({
      data: JSON.stringify({
        arg: { channel: 'books', instId: market.id },
        action: 'snapshot',
        data: [{ bids: [], asks: [], seqId: 1, prevSeqId: -1, instId: 'ETH-USDT-SWAP' }],
      }),
    } as MessageEvent);

    expect(matchingBook).toHaveBeenCalledOnce();
    expect(wrongBook).not.toHaveBeenCalled();
    expect(Object.keys(websocket.messageHandlersStats)).toEqual([`books.${market.id}`]);
  });

  it('maps an exact business candle topic back to its internal timeframe handler', () => {
    exchange = createExchange();
    const websocket = exchange.businessWebsocket;
    const matchingCandle = vi.fn();
    const wrongCandle = vi.fn();

    websocket.messageHandlers = {
      [`candle.1h.${market.id}`]: [matchingCandle],
      'candle.1h.ETH-USDT-SWAP': [wrongCandle],
    };

    const frame = {
      arg: { channel: 'candle1H', instId: market.id },
      data: [['1700000000000', '1', '2', '0.5', '1.5', '0', '0', '12']],
    };
    websocket.onMessage({ data: JSON.stringify(frame) } as MessageEvent);
    websocket.onMessage({ data: JSON.stringify({ event: 'subscribe', ...frame }) } as MessageEvent);

    expect(matchingCandle).toHaveBeenCalledOnce();
    expect(wrongCandle).not.toHaveBeenCalled();
    expect(Object.keys(websocket.messageHandlersStats)).toEqual([`candle.1h.${market.id}`]);
  });

  it('uses exact business arg channels for algo orders and ignores misleading payload fields', () => {
    exchange = createExchange();
    const websocket = exchange.businessWebsocket;
    const candle = vi.fn();
    const handleOrderTopic = vi.spyOn(websocket, 'handleOrderTopic').mockImplementation(() => {});
    websocket.messageHandlers = { [`candle.1m.${market.id}`]: [candle] };

    websocket.onMessage({
      data: JSON.stringify({
        arg: { channel: 'candle1m', instId: market.id },
        data: [{ channel: 'orders-algo', instType: 'SWAP' }],
      }),
    } as MessageEvent);
    websocket.onMessage({
      data: JSON.stringify({
        arg: { channel: 'orders-algo', instType: 'SWAP' },
        data: [{ instId: market.id }],
      }),
    } as MessageEvent);

    expect(candle).toHaveBeenCalledOnce();
    expect(handleOrderTopic).toHaveBeenCalledOnce();
    // Every order channel records under one stats topic on purpose: OKX only
    // pushes the channel for a category the account currently holds orders in,
    // so per-category entries went quiet independently as categories emptied.
    expect(Object.keys(websocket.messageHandlersStats).sort()).toEqual([
      `candle.1m.${market.id}`,
      ORDER_STATS_TOPIC,
    ]);
  });

  it('subscribes only after an exact successful login event', () => {
    exchange = createExchange();
    const websocket = exchange.businessWebsocket;
    const subscribe = vi.spyOn(websocket, 'subscribe').mockImplementation(() => {});

    websocket.onMessage({ data: JSON.stringify({ event: 'notice', msg: 'login', code: '0' }) } as MessageEvent);
    websocket.onMessage({ data: JSON.stringify({ event: 'login', code: '0' }) } as MessageEvent);

    expect(subscribe).toHaveBeenCalledOnce();
  });
});
