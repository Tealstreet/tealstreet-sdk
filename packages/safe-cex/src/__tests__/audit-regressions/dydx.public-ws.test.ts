import { describe, expect, it, vi } from 'vitest';

import { DydxExchange } from '../../exchanges/dydx/dydx.exchange';
import { WS_CHANNELS } from '../../exchanges/dydx/dydx.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide } from '../../types';

const symbol = 'BTC-USD';

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: symbol,
        symbol,
        base: 'BTC',
        quote: 'USD',
        active: true,
        precision: { amount: 0.001, price: 1 },
        limits: { amount: { min: 0.001, max: 1_000_000 }, leverage: { min: 1, max: 20 } },
        contractSize: 1,
        takerFee: 0,
        makerFee: 0,
      },
    ],
    tickers: [
      {
        id: symbol,
        symbol,
        bid: 0,
        ask: 0,
        last: 0,
        mark: 0,
        index: 0,
        percentage: 0,
        fundingRate: 0,
        fundingTime: 0,
        volume: 0,
        quoteVolume: 0,
        openInterest: 0,
      },
    ],
  });

  const exchange = new DydxExchange('dydx', { publicDataOnly: true } as any, store);
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  const frames: string[] = [];
  exchange.publicWebsocket.ws = {
    readyState: WebSocket.OPEN,
    send: (frame: string) => frames.push(frame),
  } as unknown as WebSocket;

  return { exchange, store, websocket: exchange.publicWebsocket, frames };
};

const message = (value: unknown) => ({ data: JSON.stringify(value) }) as MessageEvent;

describe('dYdX public WebSocket audit regressions', () => {
  it('builds the subscribed object snapshot and merges tuple deltas including removals', () => {
    const { websocket } = createExchange();
    const callback = vi.fn();
    websocket.listenOrderBook(symbol, callback);

    websocket.onMessage(
      message({
        type: 'subscribed',
        channel: WS_CHANNELS.ORDERBOOK,
        id: symbol,
        contents: {
          bids: [
            { price: '60000', size: '1' },
            { price: '59900', size: '2' },
          ],
          asks: [
            { price: '60100', size: '3' },
            { price: '60200', size: '4' },
          ],
        },
      }),
    );

    expect(callback).toHaveBeenLastCalledWith({
      bids: [
        { price: 60000, amount: 1, total: 1 },
        { price: 59900, amount: 2, total: 3 },
      ],
      asks: [
        { price: 60100, amount: 3, total: 3 },
        { price: 60200, amount: 4, total: 7 },
      ],
    });

    websocket.onMessage(
      message({
        type: 'channel_batch_data',
        channel: WS_CHANNELS.ORDERBOOK,
        id: symbol,
        contents: [{ bids: [['60000', '1.5']], asks: [['60100', '0']] }],
      }),
    );

    expect(callback).toHaveBeenCalledTimes(2);
    expect(callback).toHaveBeenLastCalledWith({
      bids: [
        { price: 60000, amount: 1.5, total: 1.5 },
        { price: 59900, amount: 2, total: 3.5 },
      ],
      asks: [{ price: 60200, amount: 4, total: 4 }],
    });
  });

  it('uses the message ticker for trades and updates ticker.last', () => {
    const { exchange, store, websocket } = createExchange();
    const onFill = vi.fn();
    exchange.emitter.on('fill', onFill);

    websocket.onMessage(
      message({
        type: 'channel_batch_data',
        channel: WS_CHANNELS.TRADES,
        id: symbol,
        contents: [
          {
            trades: [
              {
                id: 'trade-uuid-not-a-market',
                side: 'BUY',
                price: '60001',
                size: '0.25',
                type: 'LIMIT',
                createdAt: '2026-07-15T00:00:00.000Z',
                createdAtHeight: '60000000',
              },
            ],
          },
        ],
      }),
    );

    expect(store.tickersById[symbol].last).toBe(60001);
    expect(onFill).toHaveBeenCalledWith({
      id: 'trade-uuid-not-a-market',
      side: OrderSide.Buy,
      symbol,
      price: 60001,
      amount: 0.25,
    });
  });

  it('replays active orderbook and candle topics after reconnect', () => {
    const { websocket, frames } = createExchange();
    websocket.topics[`${WS_CHANNELS.ORDERBOOK}.${symbol}`] = 1;
    websocket.topics[`${WS_CHANNELS.CANDLES}.${symbol}.1MIN`] = 1;

    websocket.onOpen();

    expect(frames.map((frame) => JSON.parse(frame))).toEqual(
      expect.arrayContaining([
        { type: 'subscribe', channel: WS_CHANNELS.MARKETS },
        { type: 'subscribe', channel: WS_CHANNELS.BLOCK_HEIGHT },
        { type: 'subscribe', channel: WS_CHANNELS.TRADES, id: symbol, batched: true },
        { type: 'subscribe', channel: WS_CHANNELS.ORDERBOOK, id: symbol, batched: true },
        { type: 'subscribe', channel: WS_CHANNELS.CANDLES, id: `${symbol}/1MIN`, batched: true },
      ]),
    );
  });
});
