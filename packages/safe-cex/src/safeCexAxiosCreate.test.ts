import type { BaseExchange } from './exchanges/base';
import type { ExchangeOptions } from './types';

import axios from 'axios';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import './transport/registerTealstreetTransport';

import { getProxyToken } from './accessToken';
// Import after mocking
import { httpStats, LAST_LOG_PERIODICY, safeCexAxiosCreate, safeCexAxiosCreateNoProxy } from './safeCexAxiosCreate';
import { getProxyForAxios, PROXY_TOKEN_WAIT_MS } from './transport/tealstreet/tealstreetTransport';

// Mock dependencies
vi.mock('axios', () => ({
  default: {
    create: vi.fn(() => ({
      interceptors: {
        request: {
          use: vi.fn(),
        },
        response: {
          use: vi.fn(),
        },
      },
    })),
  },
}));

vi.mock('./accessToken', async (importOriginal) => {
  const getProxyToken = vi.fn(() => Promise.resolve('mock-token'));
  return {
    ...(await importOriginal<typeof import('./accessToken')>()),
    getProxyToken,
    // Read by configureHOW, which the Tealstreet transport pulls in at import.
    isTokenExpired: vi.fn(() => false),
    // The real binder looks the token up through getProxyToken.
    bindProxyTokenLookup: () => () => getProxyToken(),
  };
});

vi.mock('./utils/axiosErrorEnhancer', () => ({
  enhanceAxiosError: vi.fn((error) => error),
}));

vi.mock('./utils/isDevelopment', () => ({
  isDevelopment: false,
}));

describe('safeCexAxiosCreate', () => {
  // getProxyForAxios resolves the public CORS host via getPublicCorsProxy().

  describe('LAST_LOG_PERIODICY', () => {
    it('should be 30 seconds in milliseconds', () => {
      expect(LAST_LOG_PERIODICY).toBe(30000);
    });
  });

  describe('httpStats', () => {
    beforeEach(() => {
      // Reset the httpStats state by creating a clean tracker
      // Since httpStats is a singleton, we can test it directly
    });

    it('should track requests per exchange', () => {
      const initialStats = httpStats.getStats();
      const initialTotal = initialStats.total;

      httpStats.recordRequest('binance');
      httpStats.recordRequest('binance');
      httpStats.recordRequest('bybit');

      const stats = httpStats.getStats();
      expect(stats.total).toBe(initialTotal + 3);
    });

    it('should track errors', () => {
      const initialStats = httpStats.getStats();
      const initialErrors = initialStats.errors;

      httpStats.recordError();
      httpStats.recordError();

      const stats = httpStats.getStats();
      expect(stats.errors).toBe(initialErrors + 2);
    });

    it('should return requests per minute', () => {
      const perMinute = httpStats.getRequestsPerMinute();
      expect(typeof perMinute).toBe('number');
      expect(perMinute).toBeGreaterThanOrEqual(0);
    });

    it('should return stats object with expected properties', () => {
      const stats = httpStats.getStats();
      expect(stats).toHaveProperty('perMinute');
      expect(stats).toHaveProperty('total');
      expect(stats).toHaveProperty('errors');
      expect(typeof stats.perMinute).toBe('number');
      expect(typeof stats.total).toBe('number');
      expect(typeof stats.errors).toBe('number');
    });
  });

  describe('getProxyForAxios', () => {
    beforeEach(() => {
      vi.clearAllMocks();
    });

    // Both pins were AWS's exchange-proxy.jp; PUBLIC_CORS_PROXY moved to the edge
    // Worker's /api/public_proxy, which never reaches the whitelisted fleet (AWS_EXIT.md 3d).
    it('returns the public CORS proxy for binance public data (migration off)', () => {
      const config = { baseURL: 'https://api.binance.com' };
      const options: ExchangeOptions = {
        key: '',
        secret: '',
        publicDataOnly: true,
      };

      const result = getProxyForAxios(config, options, 'binance');
      expect(result).toBe('https://fsjnpy143pbz-tyo.tealstreet.io/api/public_proxy');
    });

    it('returns the public CORS proxy for binance without an API key', () => {
      const config = { baseURL: 'https://api.binance.com', headers: {} };
      const options: ExchangeOptions = {
        key: '',
        secret: '',
        publicDataOnly: false,
      };

      const result = getProxyForAxios(config, options, 'binance');
      expect(result).toBe('https://fsjnpy143pbz-tyo.tealstreet.io/api/public_proxy');
    });

    it('should return private proxy URL when tealstreetProxy is enabled', () => {
      const config = { baseURL: 'https://api.bybit.com' };
      const options: ExchangeOptions = {
        key: 'test-key',
        secret: 'test-secret',
        tealstreetProxy: true,
      };

      const result = getProxyForAxios(config, options, 'bybit');
      // Every pool is the one Vultr Worker since AWS_EXIT.md step 3e (was fsjnpy143pbz.asia).
      expect(result).toBe('https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy');
    });

    it('should return private proxy URL for Japan region', () => {
      const config = { baseURL: 'https://api.bybit.com' };
      const options: ExchangeOptions = {
        key: 'test-key',
        secret: 'test-secret',
        tealstreetProxy: true,
        proxyRegion: 'JA',
      };

      const result = getProxyForAxios(config, options, 'bybit');
      // Was fsjnpy143pbz.jp (AWS) until AWS_EXIT.md step 3e.
      expect(result).toBe('https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy');
    });

    it('should return empty string when no proxy is needed', () => {
      const config = { baseURL: 'https://api.bybit.com' };
      const options: ExchangeOptions = {
        key: 'test-key',
        secret: 'test-secret',
        tealstreetProxy: false,
      };

      const result = getProxyForAxios(config, options, 'bybit');
      expect(result).toBe('');
    });

    it('should not use exchange proxy for binance with API key', () => {
      const config = {
        baseURL: 'https://api.binance.com',
        headers: { 'X-MBX-APIKEY': 'some-api-key' },
      };
      const options: ExchangeOptions = {
        key: 'test-key',
        secret: 'test-secret',
        publicDataOnly: false,
      };

      const result = getProxyForAxios(config, options, 'binance');
      expect(result).toBe('');
    });

    it('should handle undefined baseURL', () => {
      const config = { baseURL: undefined };
      const options: ExchangeOptions = {
        key: '',
        secret: '',
        publicDataOnly: true,
      };

      const result = getProxyForAxios(config, options, 'bybit');
      expect(result).toBe('');
    });
  });

  describe('safeCexAxiosCreateNoProxy', () => {
    let mockExchange: BaseExchange;

    beforeEach(() => {
      vi.clearAllMocks();
      mockExchange = {
        name: 'testexchange',
        emitter: { emit: vi.fn() },
        grafanaEndpointsEvery: [],
        grafanaEndpointsPeriodic: [],
      } as unknown as BaseExchange;
    });

    it('should create axios instance with default timeout', () => {
      const config = { baseURL: 'https://api.test.com' };
      const options: ExchangeOptions = { key: '', secret: '' };

      safeCexAxiosCreateNoProxy(config, options, mockExchange);

      expect(axios.create).toHaveBeenCalledWith({
        ...config,
        timeout: 20_000,
      });
    });

    it('should add request interceptor', () => {
      const config = { baseURL: 'https://api.test.com' };
      const options: ExchangeOptions = { key: '', secret: '' };

      const instance = safeCexAxiosCreateNoProxy(config, options, mockExchange);

      expect(instance.interceptors.request.use).toHaveBeenCalled();
    });

    it('should add response interceptor', () => {
      const config = { baseURL: 'https://api.test.com' };
      const options: ExchangeOptions = { key: '', secret: '' };

      const instance = safeCexAxiosCreateNoProxy(config, options, mockExchange);

      expect(instance.interceptors.response.use).toHaveBeenCalled();
    });

    it('should return axios instance', () => {
      const config = { baseURL: 'https://api.test.com' };
      const options: ExchangeOptions = { key: '', secret: '' };

      const instance = safeCexAxiosCreateNoProxy(config, options, mockExchange);

      expect(instance).toBeDefined();
      expect(instance.interceptors).toBeDefined();
    });
  });

  describe('safeCexAxiosCreate', () => {
    let mockExchange: BaseExchange;

    beforeEach(() => {
      vi.clearAllMocks();
      mockExchange = {
        name: 'testexchange',
        emitter: { emit: vi.fn() },
        grafanaEndpointsEvery: [],
        grafanaEndpointsPeriodic: [],
      } as unknown as BaseExchange;
    });

    it('should create axios instance with default timeout', () => {
      const config = { baseURL: 'https://api.test.com' };
      const options: ExchangeOptions = { key: '', secret: '' };

      safeCexAxiosCreate(config, options, mockExchange);

      expect(axios.create).toHaveBeenCalledWith(
        expect.objectContaining({
          timeout: 20_000,
        }),
      );
    });

    it('should use overrideProxyUrl when provided', () => {
      const config = { baseURL: 'https://api.test.com' };
      const options: ExchangeOptions = {
        key: '',
        secret: '',
        overrideProxyUrl: 'https://custom-proxy.com',
      };

      safeCexAxiosCreate(config, options, mockExchange);

      expect(axios.create).toHaveBeenCalledWith(
        expect.objectContaining({
          baseURL: 'https://custom-proxy.com/https://api.test.com',
        }),
      );
    });

    it('should add multiple request interceptors', () => {
      const config = { baseURL: 'https://api.test.com' };
      const options: ExchangeOptions = { key: '', secret: '' };

      const instance = safeCexAxiosCreate(config, options, mockExchange);

      // At least 3 request interceptors: stats tracking, proxy auth, logging
      expect(instance.interceptors.request.use).toHaveBeenCalledTimes(3);
    });

    it('uses Tealstreet-Authorization for MEXC proxy auth to avoid venue Authorization conflicts', async () => {
      const mexcExchange = {
        ...mockExchange,
        name: 'mexc',
      } as unknown as BaseExchange;
      const config = { baseURL: 'https://api.mexc.com' };
      const options: ExchangeOptions = {
        key: 'test-key',
        secret: 'test-secret',
        tealstreetProxy: true,
      };

      const instance = safeCexAxiosCreate(config, options, mexcExchange);
      const proxyAuthInterceptor = vi.mocked(instance.interceptors.request.use).mock.calls[1][0];
      expect(proxyAuthInterceptor).toBeDefined();
      const requestConfig = await proxyAuthInterceptor!({ headers: {} } as any);

      expect(requestConfig.headers['Tealstreet-Authorization']).toBe('Bearer mock-token');
      expect(requestConfig.headers.Authorization).toBeUndefined();
    });

    // A warming token ('' from the SharedWorker or CLI) must never reach the
    // edge: no header is a 401 that reads as a refused venue key, and an empty
    // `Bearer ` is a malformed credential. The request waits for a token instead.
    it('waits for a warming proxy token and sends it, never an absent or empty credential', async () => {
      vi.useFakeTimers();
      try {
        vi.mocked(getProxyToken).mockResolvedValueOnce('').mockResolvedValueOnce('');

        const bybitExchange = { ...mockExchange, name: 'bybit' } as unknown as BaseExchange;
        const config = { baseURL: 'https://api.bybit.com' };
        const options: ExchangeOptions = { key: 'k', secret: 's', tealstreetProxy: true };

        const instance = safeCexAxiosCreate(config, options, bybitExchange);
        const proxyAuthInterceptor = vi.mocked(instance.interceptors.request.use).mock.calls[1][0];
        const pending = proxyAuthInterceptor!({ headers: {} } as any);
        await vi.advanceTimersByTimeAsync(1_000);
        const requestConfig = await pending;

        expect(requestConfig.headers.Authorization).toBe('Bearer mock-token');
        expect((requestConfig as unknown as Record<string, unknown>).safeCexRestDiagProxyHeader).toBe('present');
        expect(getProxyToken).toHaveBeenCalledTimes(3);
      } finally {
        vi.useRealTimers();
      }
    });

    it('refuses once the bounded wait is over, naming the missing Tealstreet proxy authorization', async () => {
      vi.useFakeTimers();
      try {
        vi.mocked(getProxyToken).mockResolvedValue('');

        const bybitExchange = { ...mockExchange, name: 'bybit' } as unknown as BaseExchange;
        const config = { baseURL: 'https://api.bybit.com' };
        const options: ExchangeOptions = { key: 'k', secret: 's', tealstreetProxy: true };

        const instance = safeCexAxiosCreate(config, options, bybitExchange);
        const proxyAuthInterceptor = vi.mocked(instance.interceptors.request.use).mock.calls[1][0];
        const refused = expect(proxyAuthInterceptor!({ headers: {} } as any)).rejects.toMatchObject({
          name: 'ProxyTokenUnavailableError',
          message: expect.stringContaining('[bybit] Tealstreet proxy authorization missing'),
        });
        await vi.advanceTimersByTimeAsync(PROXY_TOKEN_WAIT_MS + 1_000);
        await refused;
      } finally {
        vi.mocked(getProxyToken).mockResolvedValue('mock-token');
        vi.useRealTimers();
      }
    });

    it('stamps the request as carrying proxy auth when the token is present', async () => {
      const bybitExchange = { ...mockExchange, name: 'bybit' } as unknown as BaseExchange;
      const config = { baseURL: 'https://api.bybit.com' };
      const options: ExchangeOptions = { key: 'k', secret: 's', tealstreetProxy: true };

      const instance = safeCexAxiosCreate(config, options, bybitExchange);
      const proxyAuthInterceptor = vi.mocked(instance.interceptors.request.use).mock.calls[1][0];
      const requestConfig = await proxyAuthInterceptor!({ headers: {} } as any);

      expect(requestConfig.headers.Authorization).toBe('Bearer mock-token');
      expect((requestConfig as unknown as Record<string, unknown>).safeCexRestDiagProxyHeader).toBe('present');
    });

    it('should add response interceptor', () => {
      const config = { baseURL: 'https://api.test.com' };
      const options: ExchangeOptions = { key: '', secret: '' };

      const instance = safeCexAxiosCreate(config, options, mockExchange);

      expect(instance.interceptors.response.use).toHaveBeenCalled();
    });

    it('labels proxy-generated auth failures as Tealstreet proxy auth failures', async () => {
      const bybitExchange = {
        ...mockExchange,
        name: 'bybit',
      } as unknown as BaseExchange;
      const config = { baseURL: 'https://api.bybit.com' };
      const options: ExchangeOptions = {
        key: 'test-key',
        secret: 'test-secret',
        tealstreetProxy: true,
      };
      const instance = safeCexAxiosCreate(config, options, bybitExchange);
      const responseErrorInterceptor = vi.mocked(instance.interceptors.response.use).mock.calls[0][1];
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
      const error = {
        response: {
          status: 401,
          data: 'Authorization header is missing',
        },
        config: {
          method: 'post',
          url: '/v5/order/create',
        },
      };

      try {
        await expect(responseErrorInterceptor!(error)).rejects.toBe(error);
      } finally {
        consoleError.mockRestore();
      }

      const expectedMessage = '[bybit] Tealstreet proxy auth failed (401): Authorization header is missing';
      expect(bybitExchange.emitter.emit).toHaveBeenCalledWith('error', expectedMessage);
    });

    it('labels forwarded GRVT auth failures as upstream exchange auth failures via proxy', async () => {
      const grvtExchange = {
        ...mockExchange,
        name: 'grvt',
      } as unknown as BaseExchange;
      const config = { baseURL: 'https://trades.grvt.io' };
      const options: ExchangeOptions = {
        key: 'test-key',
        secret: 'test-secret',
        tealstreetProxy: true,
      };
      const instance = safeCexAxiosCreate(config, options, grvtExchange);
      const responseErrorInterceptor = vi.mocked(instance.interceptors.response.use).mock.calls[0][1];
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
      const error = {
        response: {
          status: 401,
          data: {
            c: 1000,
            m: 'You need to authenticate prior to using this functionality',
            s: 401,
          },
        },
        config: {
          method: 'post',
          url: '/lite/v1/aggregated_account_summary',
        },
      };

      try {
        await expect(responseErrorInterceptor!(error)).rejects.toBe(error);
      } finally {
        consoleError.mockRestore();
      }

      const expectedMessage =
        '[grvt] Upstream exchange auth failed via proxy (401): {"c":1000,"m":"You need to authenticate prior to using this functionality","s":401}';
      expect(grvtExchange.emitter.emit).toHaveBeenCalledWith('error', expectedMessage);
    });

    describe('GRVT exchange special handling', () => {
      it('should enable proxy for GRVT when tealstreetTunnel is true', () => {
        const grvtExchange = {
          ...mockExchange,
          name: 'grvt',
        } as unknown as BaseExchange;
        const config = { baseURL: 'https://api.grvt.io' };
        const options: ExchangeOptions = {
          key: '',
          secret: '',
          tealstreetTunnel: true,
        };

        safeCexAxiosCreate(config, options, grvtExchange);

        // GRVT routes through the private proxy even with tunnel (effectiveOptions forces tealstreetProxy).
        const grvtBaseURL = vi.mocked(axios.create).mock.calls.at(-1)?.[0]?.baseURL;
        expect(grvtBaseURL).toContain('fsjnpy143pbz');
      });
    });

    describe('BloFin exchange special handling', () => {
      it('should enable proxy for blofin when tealstreetTunnel is true', () => {
        const blofinExchange = {
          ...mockExchange,
          name: 'blofin',
        } as unknown as BaseExchange;
        const config = { baseURL: 'https://api.blofin.com' };
        const options: ExchangeOptions = {
          key: '',
          secret: '',
          tealstreetTunnel: true,
        };

        safeCexAxiosCreate(config, options, blofinExchange);

        // BloFin routes through the private proxy even with tunnel (effectiveOptions forces tealstreetProxy).
        const blofinBaseURL = vi.mocked(axios.create).mock.calls.at(-1)?.[0]?.baseURL;
        expect(blofinBaseURL).toContain('fsjnpy143pbz');
      });
    });

    it('should return axios instance', () => {
      const config = { baseURL: 'https://api.test.com' };
      const options: ExchangeOptions = { key: '', secret: '' };

      const instance = safeCexAxiosCreate(config, options, mockExchange);

      expect(instance).toBeDefined();
      expect(instance.interceptors).toBeDefined();
    });

    it('should not use proxy URL when tealstreetTunnel is true for non-special exchanges', () => {
      const config = { baseURL: 'https://api.bybit.com' };
      const options: ExchangeOptions = {
        key: '',
        secret: '',
        tealstreetTunnel: true,
        tealstreetProxy: true,
      };

      vi.clearAllMocks();
      safeCexAxiosCreate(config, options, mockExchange);

      // For regular exchanges with tunnel, proxy should not be used
      expect(axios.create).toHaveBeenCalledWith(
        expect.objectContaining({
          baseURL: 'https://api.bybit.com',
        }),
      );
    });
  });
});
