import type { ExchangeOptions, Market, OrderBook } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { OKXExchange } from '../reference/okx/okx.exchange';
import { DefaultStore } from '../../store/store.base';
import { SequencedDeltaOrderBook } from '../../utils/sequencedDeltaOrderBook';

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
  contractSize: 1,
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

const frame = (action: 'snapshot' | 'update', seqId: string, prevSeqId: string, bids: string[][] = []) =>
  JSON.stringify({
    arg: { channel: 'books', instId: market.id },
    action,
    data: [{ bids, asks: [], seqId, prevSeqId, checksum: 0 }],
  });

describe('OKX sequenced orderbook adoption gate', () => {
  it('preserves OKX maintenance resets and empty keepalives without resubscribing', () => {
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    websocket.ws = { readyState: WebSocket.OPEN } as WebSocket;
    const send = vi.spyOn(websocket, 'send').mockImplementation(() => {});
    const callback = vi.fn();

    websocket.listenOrderBook(market.symbol, callback);
    websocket.onMessage({ data: frame('snapshot', '15', '-1', [['99', '1']]) } as MessageEvent);
    websocket.onMessage({ data: frame('update', '15', '15') } as MessageEvent);
    websocket.onMessage({ data: frame('update', '3', '15', [['98', '2']]) } as MessageEvent);
    websocket.onMessage({ data: frame('update', '5', '3', [['97', '3']]) } as MessageEvent);

    expect(callback).toHaveBeenCalledTimes(4);
    expect(callback.mock.calls.at(-1)?.[0].bids).toEqual([
      { price: 99, amount: 1, total: 1 },
      { price: 98, amount: 2, total: 3 },
      { price: 97, amount: 3, total: 6 },
    ]);
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith(
      JSON.stringify({ op: 'subscribe', args: [{ channel: 'books', instId: market.id }] }),
    );
  });

  it('documents why the monotonic shared helper cannot yet replace the OKX gate', () => {
    const orderBook: OrderBook = { bids: [], asks: [] };
    const sequenced = new SequencedDeltaOrderBook(orderBook);

    sequenced.applySnapshot({ sequence: '15', bids: [[99, 1]], asks: [] });

    expect(
      sequenced.applyDelta({
        sequence: '3',
        previousSequence: '15',
        bids: [[98, 2]],
        asks: [],
      }),
    ).toEqual({ status: 'ignored', reason: 'stale', sequence: '3' });
    expect(orderBook.bids).toEqual([{ price: 99, amount: 1, total: 0 }]);
  });

  it.each([
    { name: 'replayed', sequence: '105', previousSequence: '100' },
    { name: 'stale', sequence: '90', previousSequence: '80' },
  ])('fails closed with one resubscribe for a $name update', ({ sequence, previousSequence }) => {
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    websocket.ws = { readyState: WebSocket.OPEN } as WebSocket;
    const send = vi.spyOn(websocket, 'send').mockImplementation(() => {});
    const callback = vi.fn();

    websocket.listenOrderBook(market.symbol, callback);
    websocket.onMessage({ data: frame('snapshot', '100', '-1', [['99', '1']]) } as MessageEvent);
    websocket.onMessage({ data: frame('update', '105', '100', [['98', '2']]) } as MessageEvent);
    websocket.onMessage({ data: frame('update', sequence, previousSequence, [['98', '2']]) } as MessageEvent);
    websocket.onMessage({ data: frame('update', '106', '105', [['97', '3']]) } as MessageEvent);

    expect(callback).toHaveBeenCalledTimes(2);
    expect(send.mock.calls.map(([payload]) => payload)).toEqual([
      JSON.stringify({ op: 'subscribe', args: [{ channel: 'books', instId: market.id }] }),
      JSON.stringify({ op: 'unsubscribe', args: [{ channel: 'books', instId: market.id }] }),
      JSON.stringify({ op: 'subscribe', args: [{ channel: 'books', instId: market.id }] }),
    ]);
  });
});
