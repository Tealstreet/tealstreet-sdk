import { describe, expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { createAPI } from '../../exchanges/bybit/bybit.api';
import { virtualClock } from '../../utils/virtualClock';

/**
 * A known-bad bybit key was polled at the 500ms metadata tick interval forever:
 * the e2e "Bad API Key — error message and bounded reconnects" test counted 120
 * errors in a 30s window against a bound of 100, and the count was purely a
 * function of how long you watched. Nothing anywhere backed off.
 */
describe('auth failure tick backoff', () => {
  // getTickInterval is protected; the streak accessor is the public surface.
  const makeExchange = (tickInterval: number) => {
    const exchange = Object.create(BaseExchange.prototype) as BaseExchange & { tickInterval(): number | undefined };
    (exchange as any).options = { extra: { tickInterval } };
    (exchange as any)._exchangeName = 'bybit';
    (exchange as any)._authFailureStreak = 0;
    return Object.assign(exchange, {
      tickInterval: () => (BaseExchange.prototype as any).getTickInterval.call(exchange),
    });
  };

  it('leaves the interval untouched while auth is healthy', () => {
    const exchange = makeExchange(500);
    expect(exchange.tickInterval()).toBe(500);
    expect(exchange.authFailureStreak).toBe(0);
  });

  it('doubles the interval per consecutive auth failure', () => {
    const exchange = makeExchange(500);

    exchange.recordAuthFailure();
    expect(exchange.tickInterval()).toBe(1000);

    exchange.recordAuthFailure();
    expect(exchange.tickInterval()).toBe(2000);

    exchange.recordAuthFailure();
    expect(exchange.tickInterval()).toBe(4000);
  });

  it('caps the backoff so recovery latency stays bounded', () => {
    const exchange = makeExchange(500);
    for (let i = 0; i < 50; i += 1) exchange.recordAuthFailure();

    expect(exchange.tickInterval()).toBe(60_000);
    expect(exchange.authFailureStreak).toBe(7);
  });

  it('resets on the first success so a fixed key recovers without a reconnect', () => {
    const exchange = makeExchange(500);
    exchange.recordAuthFailure();
    exchange.recordAuthFailure();
    expect(exchange.tickInterval()).toBe(2000);

    exchange.recordAuthSuccess();
    expect(exchange.tickInterval()).toBe(500);
    expect(exchange.authFailureStreak).toBe(0);
  });

  it('bounds requests in a 30s window far below the old ~120', () => {
    const exchange = makeExchange(500);
    let elapsed = 0;
    let requests = 0;

    while (elapsed < 30_000) {
      requests += 1;
      exchange.recordAuthFailure();
      elapsed += exchange.tickInterval() as number;
    }

    expect(requests).toBeLessThanOrEqual(8);
  });
});

/**
 * The HOW adapter resolves non-2xx without axios' settle(), so an auth failure
 * arrives at the response interceptor as a SUCCESS with `{"raw_response":""}`.
 * Callers then emitted `undefined` -> 'Unknown Exchange Error'.
 */
describe('bybit response interceptor auth handling', () => {
  const makeApi = () => {
    const exchange = {
      name: 'bybit',
      recordAuthFailure: vi.fn(),
      recordAuthSuccess: vi.fn(),
      emitAndLogError: vi.fn(),
    } as unknown as BaseExchange;

    const xhr = createAPI({ key: 'k', secret: 's' } as any, exchange);
    // safeCexAxiosCreate registers its own response interceptor first; ours is
    // appended, so take the last registered handler rather than index 0.
    const handlers = (xhr.interceptors.response as any).handlers.filter(Boolean);
    const handler = handlers[handlers.length - 1].fulfilled;
    return { exchange, handler };
  };

  const signed = { headers: { 'X-BAPI-SIGN': 'abc' } };
  const clockReject = {
    retCode: 10002,
    retMsg:
      'invalid request, please check your server timestamp or recv_window param: req_timestamp[1000],server_timestamp[2000],recv_window[20000]',
  };

  it('turns a tunnelled 401 into a message the auth patterns match', async () => {
    const { exchange, handler } = makeApi();

    const res = await handler({ status: 401, statusText: '', data: { raw_response: '' }, config: signed });

    expect(res.data.retCode).toBe(401);
    expect(res.data.retMsg).toBe('HTTP 401 Unauthorized — API key rejected');
    // The health monitor and the e2e toast assertion match words, not numbers.
    expect(res.data.retMsg).toMatch(/unauthorized/i);
    expect(res.data.retMsg).toMatch(/api.?key/i);
    expect(exchange.recordAuthFailure).toHaveBeenCalledTimes(1);
  });

  it('preserves other body fields when synthesizing', async () => {
    const { handler } = makeApi();

    const res = await handler({ status: 403, statusText: '', data: { raw_response: '', extra: 1 }, config: signed });

    expect(res.data.extra).toBe(1);
    expect(res.data.retMsg).toMatch(/forbidden/i);
  });

  it('leaves a non-auth status bare, because the reason phrase would reclassify it', async () => {
    // `response.statusText` is undefined on every tunnelled response (the frame
    // spells it `status_text`), so this degrades to a bare `HTTP 500`. That
    // looks like a bug and is not one: `errorPatterns.ts` matches 'server
    // error', so `HTTP 500 Internal Server Error` would classify exchange_down
    // and demote the red toast to a deduped log. Do NOT "fix" this by reading
    // status_text — see packages/safe-cex/CLAUDE.md.
    const { handler } = makeApi();

    // Shaped like a real tunnel frame: statusText absent, status_text present.
    const res = await handler({
      status: 500,
      status_text: 'Internal Server Error',
      data: { raw_response: '' },
      config: signed,
    });

    expect(res.data.retMsg).toBe('HTTP 500');
    expect(res.data.retMsg).not.toMatch(/server error/i);
  });

  it('counts enveloped credential failures that arrive on HTTP 200', async () => {
    const { exchange, handler } = makeApi();

    await handler({ status: 200, data: { retCode: 10003, retMsg: 'API key is invalid' }, config: signed });

    expect(exchange.recordAuthFailure).toHaveBeenCalledTimes(1);
    expect(exchange.recordAuthSuccess).not.toHaveBeenCalled();
  });

  it('does not back off on 10004 error-sign, which is usually clock drift', async () => {
    const { exchange, handler } = makeApi();

    await handler({ status: 200, data: { retCode: 10004, retMsg: 'error sign!' }, config: signed });

    expect(exchange.recordAuthFailure).not.toHaveBeenCalled();
  });

  it('only lets a SIGNED success clear the streak', async () => {
    const { exchange, handler } = makeApi();

    await handler({ status: 200, data: { retCode: 0, retMsg: 'OK' }, config: { headers: {} } });
    expect(exchange.recordAuthSuccess).not.toHaveBeenCalled();

    await handler({ status: 200, data: { retCode: 0, retMsg: 'OK' }, config: signed });
    expect(exchange.recordAuthSuccess).toHaveBeenCalledTimes(1);
  });

  it('silently resyncs and retries a Bybit clock rejection once', async () => {
    const exchange = {
      name: 'bybit',
      recordAuthFailure: vi.fn(),
      recordAuthSuccess: vi.fn(),
      emitAndLogError: vi.fn(),
    } as unknown as BaseExchange;
    const syncSpy = vi.spyOn(virtualClock, 'syncBybitClientServerTime').mockResolvedValue();
    const timeSpy = vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValueOnce(1000).mockReturnValueOnce(2000);
    const responses = [clockReject, { retCode: 0, retMsg: 'OK' }];
    const adapter = vi.fn(async (config) => ({
      config,
      data: responses.shift(),
      headers: {},
      status: 200,
      statusText: 'OK',
    }));
    const xhr = createAPI({ key: 'k', secret: 's' } as any, exchange);

    const response = await xhr.get('/v5/order/realtime', { adapter });

    expect(response.data.retMsg).toBe('OK');
    expect(adapter).toHaveBeenCalledTimes(2);
    expect(syncSpy).toHaveBeenCalledTimes(1);
    expect(exchange.emitAndLogError).not.toHaveBeenCalled();
    expect(exchange.recordAuthSuccess).toHaveBeenCalledTimes(1);
    expect(adapter.mock.calls[0][0].headers['X-BAPI-TIMESTAMP']).toBe('1000');
    expect(adapter.mock.calls[1][0].headers['X-BAPI-TIMESTAMP']).toBe('2000');
    timeSpy.mockRestore();
    syncSpy.mockRestore();
  });

  it('surfaces the clock rejection when the retry also fails', async () => {
    const exchange = {
      name: 'bybit',
      recordAuthFailure: vi.fn(),
      recordAuthSuccess: vi.fn(),
      emitAndLogError: vi.fn(),
    } as unknown as BaseExchange;
    const syncSpy = vi.spyOn(virtualClock, 'syncBybitClientServerTime').mockResolvedValue();
    const timeSpy = vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValueOnce(1000).mockReturnValueOnce(2000);
    const adapter = vi.fn(async (config) => ({
      config,
      data: clockReject,
      headers: {},
      status: 200,
      statusText: 'OK',
    }));
    const xhr = createAPI({ key: 'k', secret: 's' } as any, exchange);

    const response = await xhr.get('/v5/order/realtime', { adapter });

    expect(response.data.retMsg).toBe(clockReject.retMsg);
    expect(adapter).toHaveBeenCalledTimes(2);
    expect(syncSpy).toHaveBeenCalledTimes(1);
    expect(exchange.emitAndLogError).toHaveBeenCalledWith('Local time is out of sync after retry');
    expect(exchange.recordAuthSuccess).not.toHaveBeenCalled();
    timeSpy.mockRestore();
    syncSpy.mockRestore();
  });
});
