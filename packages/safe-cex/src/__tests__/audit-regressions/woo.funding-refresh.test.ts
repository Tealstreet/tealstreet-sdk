import type { Ticker } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { WOOXExchange } from '../reference/woo/woo.exchange';
import { ENDPOINTS } from '../reference/woo/woo.types';
import { DefaultStore } from '../../store/store.base';

type FundingRuntime = {
  refreshFundingRates: () => Promise<void>;
};

const makeTicker = (symbol: string, id = `${symbol}-id`): Ticker => ({
  id,
  symbol,
  bid: 99,
  ask: 101,
  last: 100,
  mark: 100.5,
  index: 100.25,
  percentage: 2,
  openInterest: 10_000,
  fundingRate: 0.0001,
  fundingTime: 1_700_000_000_000,
  volume: 500,
  quoteVolume: 50_000,
  info: { source: 'initial-ticker' },
});

const makeExchange = (tickers = [makeTicker('BTCUSDT')], optionOverrides: Record<string, unknown> = {}) => {
  const store = new DefaultStore();
  store.update({ tickers });
  const exchange = new WOOXExchange(
    'woo' as any,
    {
      key: 'key',
      secret: 'secret',
      applicationId: 'application-id',
      ...optionOverrides,
    } as any,
    store,
  );

  return { exchange, fundingRuntime: exchange as unknown as FundingRuntime, store };
};

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('WOO legacy funding refresh', () => {
  it('maps the official all-market REST fields without replacing the ticker or unrelated values', async () => {
    const { exchange, fundingRuntime, store } = makeExchange();
    const originalTicker = store.tickers[0];
    const originalTickerValues = { ...originalTicker };
    const get = vi.fn().mockResolvedValue({
      data: {
        success: true,
        rows: [
          {
            symbol: 'PERP_BTC_USDT',
            est_funding_rate: '-0.00003125',
            next_funding_time: 1_800_000_000_000,
          },
          {
            symbol: 'PERP_UNKNOWN_USDT',
            est_funding_rate: 0.5,
            next_funding_time: 1_800_000_000_001,
          },
        ],
        timestamp: 1_700_000_000_000,
      },
    });
    exchange.xhr = { get } as any;

    await fundingRuntime.refreshFundingRates();

    expect(get).toHaveBeenCalledOnce();
    expect(get).toHaveBeenCalledWith(ENDPOINTS.FUNDING_RATES);
    expect(store.tickers[0]).toBe(originalTicker);
    expect(store.tickers[0]).toEqual({
      ...originalTickerValues,
      fundingRate: -0.00003125,
      fundingTime: 1_800_000_000_000,
    });

    exchange.dispose();
  });

  it('retains last-known-good funding for malformed rows and malformed response shapes', async () => {
    const ethTicker = makeTicker('ETHUSDT', 'eth-id');
    const { exchange, fundingRuntime, store } = makeExchange([makeTicker('BTCUSDT'), ethTicker]);
    const emitError = vi.spyOn(exchange, 'emitAndLogError').mockReturnValue(undefined as any);
    const get = vi
      .fn()
      .mockResolvedValueOnce({
        data: {
          success: true,
          rows: [
            {
              symbol: 'PERP_BTC_USDT',
              est_funding_rate: 'not-a-number',
              next_funding_time: 1_800_000_000_000,
            },
            {
              symbol: 'PERP_ETH_USDT',
              est_funding_rate: -0.0002,
              next_funding_time: null,
            },
          ],
        },
      })
      .mockResolvedValueOnce({ data: { success: true, rows: null } });
    exchange.xhr = { get } as any;

    await fundingRuntime.refreshFundingRates();
    await fundingRuntime.refreshFundingRates();

    expect(store.tickers.map(({ fundingRate, fundingTime }) => ({ fundingRate, fundingTime }))).toEqual([
      { fundingRate: 0.0001, fundingTime: 1_700_000_000_000 },
      { fundingRate: 0.0001, fundingTime: 1_700_000_000_000 },
    ]);
    expect(emitError).toHaveBeenCalledWith('WOO X funding rates response has an invalid shape');

    exchange.dispose();
  });

  it('rejects raw symbols that only match a ticker through lossy normalization', async () => {
    const { exchange, fundingRuntime, store } = makeExchange();
    const updateTicker = vi.spyOn(store, 'updateTicker');
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          success: true,
          rows: [
            {
              symbol: 'PERP_BTC_USDT_EXTRA',
              est_funding_rate: -0.5,
              next_funding_time: 1_800_000_000_000,
            },
          ],
        },
      }),
    } as any;

    await fundingRuntime.refreshFundingRates();

    expect(updateTicker).not.toHaveBeenCalled();
    expect(store.tickers[0]).toMatchObject({
      fundingRate: 0.0001,
      fundingTime: 1_700_000_000_000,
    });

    exchange.dispose();
  });

  it('retains last-known-good funding when the REST request fails', async () => {
    const { exchange, fundingRuntime, store } = makeExchange();
    const originalTicker = store.tickers[0];
    const emitError = vi.spyOn(exchange, 'emitAndLogError').mockReturnValue(undefined as any);
    exchange.xhr = {
      get: vi.fn().mockRejectedValue({ response: { data: { message: 'temporarily unavailable' } } }),
    } as any;

    await fundingRuntime.refreshFundingRates();

    expect(store.tickers[0]).toBe(originalTicker);
    expect(store.tickers[0]).toMatchObject({
      fundingRate: 0.0001,
      fundingTime: 1_700_000_000_000,
    });
    expect(emitError).toHaveBeenCalledWith('temporarily unavailable');

    exchange.dispose();
  });

  it('runs at most once per minute, skips overlaps, and stops on dispose', async () => {
    vi.useFakeTimers();
    const { exchange, fundingRuntime } = makeExchange([makeTicker('BTCUSDT')], { publicDataOnly: true });
    vi.spyOn(exchange, 'fetchMarkets').mockResolvedValue([
      {
        id: 'BTC/USDT:USDT',
        symbol: 'BTCUSDT',
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 10 } },
      } as any,
    ]);
    vi.spyOn(exchange, 'fetchTickersRaw').mockResolvedValue([
      {
        symbol: 'PERP_BTC_USDT',
        '24h_open': 99,
        '24h_close': 100,
        index_price: 100,
        mark_price: 100,
        last_funding_rate: 0.0001,
        next_funding_time: 1_700_000_000_000,
        '24h_volume': 500,
        '24h_amount': 50_000,
        open_interest: 10_000,
      },
    ]);
    vi.spyOn(exchange.publicWebsocket, 'connectAndSubscribe').mockImplementation(() => undefined);
    let resolveFirstRequest!: (value: unknown) => void;
    const get = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirstRequest = resolve;
          }),
      )
      .mockResolvedValue({ data: { success: true, rows: [] } });
    exchange.xhr = { get } as any;

    await exchange.start();
    vi.advanceTimersByTime(59_999);
    expect(get).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(get).toHaveBeenCalledOnce();

    vi.advanceTimersByTime(60_000);
    expect(get).toHaveBeenCalledOnce();

    resolveFirstRequest({ data: { success: true, rows: [] } });
    await Promise.resolve();
    await Promise.resolve();

    await vi.advanceTimersByTimeAsync(60_000);
    expect(get).toHaveBeenCalledTimes(2);

    exchange.dispose();
    await vi.advanceTimersByTimeAsync(120_000);
    expect(get).toHaveBeenCalledTimes(2);
  });

  it('does not subscribe to the invalid global funding WebSocket topic', () => {
    const { exchange } = makeExchange();
    const send = vi.spyOn(exchange.publicWebsocket, 'send').mockImplementation(() => undefined);

    exchange.publicWebsocket.onOpen();

    const subscribedTopics = send.mock.calls
      .map(([payload]) => JSON.parse(payload as string))
      .filter(({ event }) => event === 'subscribe')
      .map(({ topic }) => topic);
    expect(subscribedTopics).toEqual(['tickers', 'bbos', 'markprices']);
    expect(subscribedTopics).not.toContain('estfundingrate');
    expect(exchange.publicWebsocket.messageHandlers).not.toHaveProperty('estfundingrate');

    exchange.dispose();
  });

  it('stops processing rows when disposal occurs reentrantly during the first ticker update', async () => {
    const { exchange, fundingRuntime, store } = makeExchange([makeTicker('BTCUSDT'), makeTicker('ETHUSDT', 'eth-id')]);
    const updateTicker = vi.spyOn(store, 'updateTicker').mockImplementationOnce(() => exchange.dispose());
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          success: true,
          rows: [
            {
              symbol: 'PERP_BTC_USDT',
              est_funding_rate: -0.0001,
              next_funding_time: 1_800_000_000_000,
            },
            {
              symbol: 'PERP_ETH_USDT',
              est_funding_rate: -0.0002,
              next_funding_time: 1_800_000_000_001,
            },
          ],
        },
      }),
    } as any;

    await fundingRuntime.refreshFundingRates();

    expect(updateTicker).toHaveBeenCalledOnce();
    expect(updateTicker).toHaveBeenCalledWith(expect.objectContaining({ symbol: 'BTCUSDT' }), {
      fundingRate: -0.0001,
      fundingTime: 1_800_000_000_000,
    });
  });

  it('delegates disposal to the base lifecycle and disposes each socket only once', () => {
    const { exchange, store } = makeExchange();
    const logListener = vi.fn();
    const publicDispose = vi.spyOn(exchange.publicWebsocket, 'dispose');
    const privateDispose = vi.spyOn(exchange.privateWebsocket, 'dispose');
    exchange.on('log', logListener);

    expect((store as any).listeners.size).toBe(1);

    exchange.dispose();
    store.update({ latency: 42 });
    exchange.emitter.emit('log', 'after-dispose');

    expect((store as any).listeners.size).toBe(0);
    expect(logListener).not.toHaveBeenCalled();
    expect(publicDispose).toHaveBeenCalledOnce();
    expect(privateDispose).toHaveBeenCalledOnce();
    expect(store.tickers).toEqual([]);
  });
});
