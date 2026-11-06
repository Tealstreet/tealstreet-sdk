import type { Axios } from 'axios';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { ENDPOINTS } from '../reference/paradex/paradex.types';
import { DefaultStore } from '../../store/store.base';

const authOptions = {
  key: '0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
  paradexAddress: '0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
  subkeyAddress: '0xcccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
  tuning: { consoleLogging: false },
} as any;

const createExchange = () => {
  const exchange = new ParadexExchange('paradex', authOptions, new DefaultStore());
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  return exchange;
};

describe('Paradex historical private auth readiness', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('retries SDK initialization before fetching historical orders', async () => {
    let initAttempts = 0;
    vi.spyOn(ParadexExchange.prototype as any, 'initializeSDK').mockImplementation(async function (this: any) {
      initAttempts += 1;
      if (initAttempts === 1) {
        throw new Error('transient sdk init failure');
      }
      this.jwtManager = { getToken: vi.fn(async () => 'token') };
    });

    const exchange = createExchange();
    const get = vi.fn(async () => ({ data: { results: [] } }));
    exchange.xhr = { get } as unknown as Axios;

    const resultPromise = exchange.fetchHistoricalOrders({ from: 1_700_000_000_000, to: 1_700_000_300_000 });
    await vi.advanceTimersByTimeAsync(500);

    await expect(resultPromise).resolves.toEqual([]);
    expect(initAttempts).toBe(2);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ORDERS_HISTORY, {
      params: {
        start_at: 1_700_000_000_000,
        end_at: 1_700_000_300_000,
        page_size: 100,
      },
    });
  });

  it('throws not-ready without emitting Sentry when historical order auth cannot initialize', async () => {
    vi.spyOn(ParadexExchange.prototype as any, 'initializeSDK').mockRejectedValue(new Error('sdk account unavailable'));

    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');
    const get = vi.fn();
    exchange.xhr = { get } as unknown as Axios;

    const resultPromise = exchange.fetchHistoricalOrders({ from: 1_700_000_000_000, to: 1_700_000_300_000 });
    const expectation = expect(resultPromise).rejects.toMatchObject({
      name: 'ParadexPrivateHistoryAuthNotReadyError',
    });
    await vi.advanceTimersByTimeAsync(2_000);

    await expectation;
    expect(emitAndLogError).not.toHaveBeenCalled();
    expect(get).not.toHaveBeenCalled();
  });
});
