import { describe, expect, it, vi } from 'vitest';

import { CryptocomExchange } from '../reference/cryptocom/cryptocom.exchange';
import { DefaultStore } from '../../store/store.base';

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTCUSD-PERP',
        symbol: 'BTCUSD-PERP',
        base: 'BTC',
        quote: 'USD',
        active: true,
        precision: { amount: 0.001, price: 0.5 },
        limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
        info: { underlyingSymbol: 'BTCUSD-INDEX' },
      },
    ],
    tickers: [
      {
        id: 'BTCUSD-PERP',
        symbol: 'BTCUSD-PERP',
        bid: 60_000,
        ask: 60_010,
        last: 60_005,
        mark: 60_005,
        index: 60_004,
        percentage: 1,
        fundingRate: 0.0001,
        fundingTime: 0,
        volume: 100,
        quoteVolume: 6_000_000,
        openInterest: 10,
      },
    ],
  });
  return new CryptocomExchange('cryptocom', { key: 'key', secret: 'secret' }, store);
};

const valuationFrame = (channel: string, value: string, instrument = 'BTCUSD-PERP') => ({
  data: JSON.stringify({
    id: 1,
    method: 'subscribe',
    code: 0,
    result: {
      channel,
      subscription: `${channel}.${instrument}`,
      instrument_name: instrument,
      data: [{ v: value, t: 1_750_000_000_000 }],
    },
  }),
});

describe('audit regression: Crypto.com public valuation streams', () => {
  it('batches ticker and valuation subscriptions on initial subscribe and reconnect', async () => {
    vi.useFakeTimers();
    try {
      const exchange = createExchange();
      const send = vi.fn();
      exchange.publicWebsocket.ws = { readyState: WebSocket.OPEN, send } as unknown as WebSocket;
      const markets = Array.from({ length: 26 }, (_, index) => ({
        instrumentName: `COIN${index}USD-PERP`,
        indexInstrumentName: `COIN${index}USD-INDEX`,
      }));

      exchange.publicWebsocket.subscribeToMarketData(markets);

      const initialFrames = send.mock.calls.map(([payload]) => JSON.parse(payload));
      expect(initialFrames).toHaveLength(2);
      expect(initialFrames.map((frame) => frame.params.channels.length)).toEqual([100, 4]);
      expect(initialFrames.flatMap((frame) => frame.params.channels)).toEqual(
        [
          ...markets.map(({ instrumentName }) => `ticker.${instrumentName}`),
          ...markets.flatMap(({ instrumentName, indexInstrumentName }) => [
            `mark.${instrumentName}`,
            `index.${indexInstrumentName}`,
            `funding.${instrumentName}`,
          ]),
        ],
      );
      expect(
        initialFrames
          .flatMap((frame) => frame.params.channels)
          .some((channel: string) => channel.startsWith('estimatedfunding.')),
      ).toBe(false);

      exchange.publicWebsocket.subscribeToMarketData(markets);
      expect(send).toHaveBeenCalledTimes(2);

      send.mockClear();
      exchange.publicWebsocket.onOpen();
      await vi.advanceTimersByTimeAsync(1_000);

      const reconnectFrames = send.mock.calls.map(([payload]) => JSON.parse(payload));
      expect(reconnectFrames).toHaveLength(2);
      expect(reconnectFrames.map((frame) => frame.params.channels.length)).toEqual([100, 4]);
    } finally {
      vi.useRealTimers();
    }
  });

  it('keeps startup market-data subscriptions under the Crypto.com session cap', () => {
    const exchange = createExchange();
    const send = vi.fn();
    exchange.publicWebsocket.ws = { readyState: WebSocket.OPEN, send } as unknown as WebSocket;
    const markets = Array.from({ length: 120 }, (_, index) => ({
      instrumentName: `COIN${index}USD-PERP`,
      indexInstrumentName: `COIN${index}USD-INDEX`,
    }));

    exchange.publicWebsocket.subscribeToMarketData(markets);

    const channels = send.mock.calls.flatMap(([payload]) => JSON.parse(payload).params.channels);
    expect(channels).toHaveLength(350);
    expect(send.mock.calls.map(([payload]) => JSON.parse(payload).params.channels.length)).toEqual([100, 100, 100, 50]);
    expect(markets.every(({ instrumentName }) => channels.includes(`ticker.${instrumentName}`))).toBe(true);
    expect(channels.some((channel: string) => channel.startsWith('estimatedfunding.'))).toBe(false);
  });

  it('maps finite positive prices and the current settling funding rate into the ticker', () => {
    const exchange = createExchange();

    exchange.publicWebsocket.onMessage(valuationFrame('mark', '60123.45') as MessageEvent);
    exchange.publicWebsocket.onMessage(valuationFrame('index', '60120.25', 'BTCUSD-INDEX') as MessageEvent);
    exchange.publicWebsocket.onMessage(valuationFrame('funding', '-0.00025') as MessageEvent);

    expect(exchange.publicWebsocket.messageHandlersStats.mark).toBeDefined();
    expect(exchange.publicWebsocket.messageHandlersStats.index).toBeDefined();
    expect(exchange.publicWebsocket.messageHandlersStats.funding).toBeDefined();
    expect(exchange.store.tickersBySymbol['BTCUSD-PERP']).toMatchObject({
      mark: 60_123.45,
      index: 60_120.25,
      fundingRate: -0.00025,
    });
  });

  it('rejects invalid valuation values and never maps next-interval estimates onto current funding', () => {
    const exchange = createExchange();

    exchange.publicWebsocket.onMessage(valuationFrame('mark', '0') as MessageEvent);
    exchange.publicWebsocket.onMessage(valuationFrame('index', 'Infinity', 'BTCUSD-INDEX') as MessageEvent);
    exchange.publicWebsocket.onMessage(valuationFrame('funding', 'NaN') as MessageEvent);
    exchange.publicWebsocket.onMessage(valuationFrame('estimatedfunding', '0.0099') as MessageEvent);

    expect(exchange.store.tickersBySymbol['BTCUSD-PERP']).toMatchObject({
      mark: 60_005,
      index: 60_004,
      fundingRate: 0.0001,
    });
  });
});
