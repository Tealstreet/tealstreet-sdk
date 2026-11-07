import { describe, expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import {
  BYBIT_ENV_MISMATCH_MESSAGE_PREFIX,
  BYBIT_ENV_PROBE_CONFIG_KEY,
  createAPI,
} from '../reference/bybit/bybit.api';
import { BASE_URL, DEMO_URL, ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { SIGNATURE_RETRY_CONFIG_KEY } from '../../utils/signatureRejection';
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

  it('keeps polling off through auth failures when the interval is Infinity', () => {
    const exchange = makeExchange(Number.POSITIVE_INFINITY);
    exchange.recordAuthFailure();

    expect(exchange.tickInterval()).toBe(Number.POSITIVE_INFINITY);
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
  const makeApi = (
    {
      exchangeName = 'bybit',
      isDemo = false,
      options = {},
      getBybitRestBaseUrl,
      probeResponseData = { retCode: 10003, retMsg: 'API key is invalid' },
      probeImplementation,
    }: {
      exchangeName?: string;
      isDemo?: boolean;
      options?: Record<string, unknown>;
      getBybitRestBaseUrl?: (network: 'livenet' | 'testnet') => string;
      probeResponseData?: unknown;
      probeImplementation?: (config: any) => Promise<any>;
    } = {},
  ) => {
    const exchange = {
      name: exchangeName,
      isDemo,
      getBybitRestBaseUrl,
      recordAuthFailure: vi.fn(),
      recordAuthSuccess: vi.fn(),
      emitAndLogError: vi.fn(),
    } as unknown as BaseExchange;

    const xhr = createAPI({ key: 'k', secret: 's', ...options } as any, exchange);
    const requestSpy = vi.spyOn(xhr, 'request').mockImplementation(
      probeImplementation ??
        (async (config) => ({
          config,
          data: probeResponseData,
          headers: {},
          status: 200,
          statusText: 'OK',
        })),
    );
    // safeCexAxiosCreate registers its own response interceptor first; ours is
    // appended, so take the last registered handler rather than index 0.
    const handlers = (xhr.interceptors.response as any).handlers.filter(Boolean);
    const handler = handlers[handlers.length - 1].fulfilled;
    return { exchange, handler, requestSpy };
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

  it('detects a demo key used against live Bybit and preserves the auth marker', async () => {
    const { exchange, handler, requestSpy } = makeApi({ probeResponseData: { retCode: 0, retMsg: 'OK' } });

    const res = await handler({ status: 403, statusText: '', data: { raw_response: '' }, config: signed });

    expect(res.data.retMsg).toMatch(new RegExp(`^${BYBIT_ENV_MISMATCH_MESSAGE_PREFIX}`));
    expect(res.data.retMsg).toContain('this key is valid for Bybit Demo Trading');
    expect(res.data.retMsg).toContain('this account is configured for live Bybit');
    expect(res.data.retMsg).toContain('HTTP 403 Forbidden — API key rejected');
    expect(requestSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        [BYBIT_ENV_PROBE_CONFIG_KEY]: true,
        baseURL: DEMO_URL.livenet,
        method: 'get',
        url: ENDPOINTS.ACCOUNT,
      }),
    );
    expect(exchange.recordAuthFailure).toHaveBeenCalledTimes(1);
    expect(exchange.recordAuthSuccess).not.toHaveBeenCalled();
  });

  it('detects a live key used against Bybit Demo', async () => {
    const { handler, requestSpy } = makeApi({
      exchangeName: 'bybitdemo',
      isDemo: true,
      probeResponseData: { retCode: 0, retMsg: 'OK' },
    });

    const res = await handler({ status: 200, data: { retCode: 10003, retMsg: 'API key is invalid' }, config: signed });

    expect(res.data.retMsg).toMatch(new RegExp(`^${BYBIT_ENV_MISMATCH_MESSAGE_PREFIX}`));
    expect(res.data.retMsg).toContain('this key is valid for live Bybit');
    expect(res.data.retMsg).toContain('this account is configured for Bybit Demo Trading');
    expect(res.data.retMsg).toContain('API key is invalid');
    expect(requestSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        [BYBIT_ENV_PROBE_CONFIG_KEY]: true,
        baseURL: BASE_URL.livenet,
        method: 'get',
        url: ENDPOINTS.ACCOUNT,
      }),
    );
  });

  it('keeps the original auth message when the opposite environment rejects the key too', async () => {
    const { handler } = makeApi();

    const res = await handler({ status: 200, data: { retCode: 10003, retMsg: 'API key is invalid' }, config: signed });

    expect(res.data.retMsg).toBe('API key is invalid');
  });

  it('does not let an environment probe affect auth-streak accounting', async () => {
    const { exchange, handler } = makeApi();

    await handler({
      status: 200,
      data: { retCode: 0, retMsg: 'OK' },
      config: { ...signed, [BYBIT_ENV_PROBE_CONFIG_KEY]: true },
    });

    expect(exchange.recordAuthFailure).not.toHaveBeenCalled();
    expect(exchange.recordAuthSuccess).not.toHaveBeenCalled();
  });

  it('dedupes concurrent auth failures through one in-flight environment probe', async () => {
    let resolveProbe!: (value: any) => void;
    const probeResponse = new Promise((resolve) => {
      resolveProbe = resolve;
    });
    const { handler, requestSpy } = makeApi({
      probeImplementation: async (config) => {
        await probeResponse;
        return {
          config,
          data: { retCode: 0, retMsg: 'OK' },
          headers: {},
          status: 200,
          statusText: 'OK',
        };
      },
    });

    const first = handler({ status: 403, statusText: '', data: { raw_response: '' }, config: signed });
    const second = handler({ status: 403, statusText: '', data: { raw_response: '' }, config: signed });
    await Promise.resolve();
    expect(requestSpy).toHaveBeenCalledTimes(1);

    resolveProbe(undefined);
    const [firstResponse, secondResponse] = await Promise.all([first, second]);

    expect(firstResponse.data.retMsg).toContain(BYBIT_ENV_MISMATCH_MESSAGE_PREFIX);
    expect(secondResponse.data.retMsg).toContain(BYBIT_ENV_MISMATCH_MESSAGE_PREFIX);
    expect(requestSpy).toHaveBeenCalledTimes(1);
  });

  it.each(
    [
      ['bybitinverse', {}, undefined],
      ['bybitspot', {}, undefined],
      ['bybit', { bybitSiteId: 'BRA_BTL' }, undefined],
      ['bybit', { testnet: true }, undefined],
      ['bybit', {}, () => 'https://example.com'],
    ] as Array<[string, Record<string, unknown>, ((network: 'livenet' | 'testnet') => string) | undefined]>,
  )('skips environment probing for scoped-out account shape %#', async (exchangeName, options, getBybitRestBaseUrl) => {
    const { handler, requestSpy } = makeApi({
      exchangeName,
      options,
      getBybitRestBaseUrl,
      probeResponseData: { retCode: 0, retMsg: 'OK' },
    });

    const res = await handler({ status: 403, statusText: '', data: { raw_response: '' }, config: signed });

    expect(res.data.retMsg).toBe('HTTP 403 Forbidden — API key rejected');
    expect(requestSpy).not.toHaveBeenCalled();
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

  // CloudFront refuses the egress address before Bybit sees the request; the key
  // was never checked, so it must not back the account off as a bad one.
  it.each([
    // What the tunnel delivers: a non-JSON body wrapped as `{ raw_response }`.
    ['tunnel-wrapped', (page: string) => ({ raw_response: page })],
    ['bare string', (page: string) => page],
  ])('does not count a CloudFront 403 as a rejected key (%s)', async (_, wrap) => {
    const { exchange, handler } = makeApi();
    const page = '<HTML><BODY><H2>The request could not be satisfied.</H2>Generated by cloudfront (CloudFront)</BODY></HTML>';

    const res = await handler({ status: 403, statusText: '', data: wrap(page), config: signed });

    expect(exchange.recordAuthFailure).not.toHaveBeenCalled();
    expect(res.data.retMsg).toBe("HTTP 403 Forbidden — blocked by Bybit's CDN before reaching Bybit");
    expect(Object.keys(res.data).every((key) => !/^\d+$/.test(key))).toBe(true);
  });

  it('counts enveloped credential failures that arrive on HTTP 200', async () => {
    const { exchange, handler } = makeApi();

    await handler({ status: 200, data: { retCode: 10003, retMsg: 'API key is invalid' }, config: signed });

    expect(exchange.recordAuthFailure).toHaveBeenCalledTimes(1);
    expect(exchange.recordAuthSuccess).not.toHaveBeenCalled();
  });

  it('counts 10004 error-sign, which is what a wrong secret returns, once the re-signed resend fails too', async () => {
    const { exchange, handler, requestSpy } = makeApi();

    await handler({ status: 200, data: { retCode: 10004, retMsg: 'error sign!' }, config: signed });
    expect(requestSpy).toHaveBeenCalledWith(expect.objectContaining({ [SIGNATURE_RETRY_CONFIG_KEY]: true }));
    expect(exchange.recordAuthFailure).not.toHaveBeenCalled();

    const resent = { ...signed, [SIGNATURE_RETRY_CONFIG_KEY]: true };
    await handler({ status: 200, data: { retCode: 10004, retMsg: 'error sign!' }, config: resent });
    expect(exchange.recordAuthFailure).toHaveBeenCalledTimes(1);
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
