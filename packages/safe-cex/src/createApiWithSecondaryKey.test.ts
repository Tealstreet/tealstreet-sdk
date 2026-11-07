import type { AxiosInstance, AxiosRequestConfig } from 'axios';
import type { BaseExchange } from './exchanges/base';
import type { ExchangeOptions } from './types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import './transport/registerTealstreetTransport';

import { createApiWithSecondaryKey } from './createApiWithSecondaryKey';

type CreateApiFn = (options: ExchangeOptions, exchange: BaseExchange) => AxiosInstance;

describe('createApiWithSecondaryKey', () => {
  let mockExchange: BaseExchange;
  let mockAxiosInstance: AxiosInstance;
  let mockSecondaryAxiosInstance: AxiosInstance;
  let mockCreateApi: CreateApiFn & { mock: { calls: [ExchangeOptions, BaseExchange][] } };
  let defaultOptions: ExchangeOptions;

  beforeEach(() => {
    mockExchange = {} as BaseExchange;

    mockAxiosInstance = {
      get: vi.fn().mockResolvedValue({ data: 'primary response' }),
      post: vi.fn().mockResolvedValue({ data: 'primary post response' }),
      put: vi.fn().mockResolvedValue({ data: 'primary put response' }),
      delete: vi.fn().mockResolvedValue({ data: 'primary delete response' }),
    } as unknown as AxiosInstance;

    mockSecondaryAxiosInstance = {
      get: vi.fn().mockResolvedValue({ data: 'secondary response' }),
      post: vi.fn().mockResolvedValue({ data: 'secondary post response' }),
      put: vi.fn().mockResolvedValue({ data: 'secondary put response' }),
      delete: vi.fn().mockResolvedValue({ data: 'secondary delete response' }),
    } as unknown as AxiosInstance;

    mockCreateApi = vi.fn((opts: ExchangeOptions) => {
      // Return secondary instance when secondary options are passed
      if (opts.secondary_key === undefined && opts.key === 'secondary-key') {
        return mockSecondaryAxiosInstance;
      }
      if (opts.overrideProxyUrl) {
        return mockSecondaryAxiosInstance;
      }
      return mockAxiosInstance;
    }) as unknown as CreateApiFn & { mock: { calls: [ExchangeOptions, BaseExchange][] } };

    defaultOptions = {
      key: 'primary-key',
      secret: 'primary-secret',
    };
  });

  describe('without secondary key', () => {
    it('should return array of three identical axios instances when no secondary_key', () => {
      const result = createApiWithSecondaryKey(defaultOptions, mockExchange, mockCreateApi);

      expect(result).toHaveLength(3);
      expect(result[0]).toBe(result[1]);
      expect(result[1]).toBe(result[2]);
    });

    it('should call createApi only once', () => {
      createApiWithSecondaryKey(defaultOptions, mockExchange, mockCreateApi);

      expect(mockCreateApi).toHaveBeenCalledTimes(1);
      expect(mockCreateApi).toHaveBeenCalledWith(defaultOptions, mockExchange);
    });

    it('should apply rate limiter to default instance', () => {
      const rateLimiter = vi.fn().mockImplementation((instance) => instance);

      createApiWithSecondaryKey(defaultOptions, mockExchange, mockCreateApi, {
        rateLimit: rateLimiter,
      });

      expect(rateLimiter).toHaveBeenCalledWith(mockAxiosInstance);
    });
  });

  describe('with secondary key', () => {
    let optionsWithSecondary: ExchangeOptions;

    beforeEach(() => {
      optionsWithSecondary = {
        key: 'primary-key',
        secret: 'primary-secret',
        secondary_key: 'secondary-key',
        secondary_secret: 'secondary-secret',
      };
    });

    it('should return array of three axios instances', () => {
      const result = createApiWithSecondaryKey(optionsWithSecondary, mockExchange, mockCreateApi);

      expect(result).toHaveLength(3);
    });

    it('should call createApi twice', () => {
      createApiWithSecondaryKey(optionsWithSecondary, mockExchange, mockCreateApi);

      expect(mockCreateApi).toHaveBeenCalledTimes(2);
    });

    it('should create secondary options with overridden key and secret', () => {
      createApiWithSecondaryKey(optionsWithSecondary, mockExchange, mockCreateApi);

      // Second call should have secondary_* fields mapped to key/secret
      const secondCallOptions = mockCreateApi.mock.calls[1][0];
      expect(secondCallOptions.key).toBe('secondary-key');
      expect(secondCallOptions.secret).toBe('secondary-secret');
    });

    it('routes the secondary read transport to the public CORS proxy', () => {
      createApiWithSecondaryKey(optionsWithSecondary, mockExchange, mockCreateApi);

      const secondCallOptions = mockCreateApi.mock.calls[1][0];
      // Was AWS's exchange-proxy.jp; moved to the edge Worker's /api/public_proxy (AWS_EXIT.md 3d).
      expect(secondCallOptions.overrideProxyUrl).toBe('https://fsjnpy143pbz-tyo.tealstreet.io/api/public_proxy');
    });

    it('should use custom proxyUrl when provided', () => {
      const customProxyUrl = 'https://custom-proxy.example.com';

      createApiWithSecondaryKey(optionsWithSecondary, mockExchange, mockCreateApi, {
        proxyUrl: customProxyUrl,
      });

      const secondCallOptions = mockCreateApi.mock.calls[1][0];
      expect(secondCallOptions.overrideProxyUrl).toBe(customProxyUrl);
    });

    it('should prefer options.secondaryProxyUrl over otherOptions.proxyUrl for the read transport', () => {
      const customSecondaryUrl = 'https://secondary-proxy.example.com/api/exchange_proxy';

      createApiWithSecondaryKey(
        { ...optionsWithSecondary, secondaryProxyUrl: customSecondaryUrl },
        mockExchange,
        mockCreateApi,
        { proxyUrl: 'https://custom-proxy.example.com' },
      );

      const secondCallOptions = mockCreateApi.mock.calls[1][0];
      expect(secondCallOptions.overrideProxyUrl).toBe(customSecondaryUrl);
    });

    it('should apply secondary rate limiter to secondary instance', () => {
      const secondaryRateLimiter = vi.fn().mockImplementation((instance) => instance);

      createApiWithSecondaryKey(optionsWithSecondary, mockExchange, mockCreateApi, {
        secondaryRateLimit: secondaryRateLimiter,
      });

      expect(secondaryRateLimiter).toHaveBeenCalledWith(mockSecondaryAxiosInstance);
    });

    it('should apply both rate limiters when provided', () => {
      const rateLimiter = vi.fn().mockImplementation((instance) => instance);
      const secondaryRateLimiter = vi.fn().mockImplementation((instance) => instance);

      createApiWithSecondaryKey(optionsWithSecondary, mockExchange, mockCreateApi, {
        rateLimit: rateLimiter,
        secondaryRateLimit: secondaryRateLimiter,
      });

      expect(rateLimiter).toHaveBeenCalledWith(mockAxiosInstance);
      expect(secondaryRateLimiter).toHaveBeenCalledWith(mockSecondaryAxiosInstance);
    });

    it('should handle secondary passphrase', () => {
      const optionsWithPassphrase: ExchangeOptions = {
        ...optionsWithSecondary,
        passphrase: 'primary-passphrase',
        secondary_passphrase: 'secondary-passphrase',
      };

      createApiWithSecondaryKey(optionsWithPassphrase, mockExchange, mockCreateApi);

      const secondCallOptions = mockCreateApi.mock.calls[1][0];
      expect(secondCallOptions.passphrase).toBe('secondary-passphrase');
    });
  });

  describe('dynamic xhr proxy behavior', () => {
    let optionsWithSecondary: ExchangeOptions;

    beforeEach(() => {
      optionsWithSecondary = {
        key: 'primary-key',
        secret: 'primary-secret',
        secondary_key: 'secondary-key',
        secondary_secret: 'secondary-secret',
      };
    });

    it('should route GET requests to secondary instance', async () => {
      const result = createApiWithSecondaryKey(optionsWithSecondary, mockExchange, mockCreateApi);
      const dynamicXhr = result[0];

      await dynamicXhr.get('/test-url');

      expect(mockSecondaryAxiosInstance.get).toHaveBeenCalledWith('/test-url', undefined);
    });

    it('should route GET requests with config to secondary instance', async () => {
      const result = createApiWithSecondaryKey(optionsWithSecondary, mockExchange, mockCreateApi);
      const dynamicXhr = result[0];
      const config: AxiosRequestConfig = { headers: { 'X-Test': 'value' } };

      await dynamicXhr.get('/test-url', config);

      expect(mockSecondaryAxiosInstance.get).toHaveBeenCalledWith('/test-url', config);
    });

    it('should route non-GET methods to primary instance', () => {
      const result = createApiWithSecondaryKey(optionsWithSecondary, mockExchange, mockCreateApi);
      const dynamicXhr = result[0];

      // Access post method - should come from primary
      expect(dynamicXhr.post).toBe(mockAxiosInstance.post);
    });

    it('should return defaultXhr as second element', () => {
      const result = createApiWithSecondaryKey(optionsWithSecondary, mockExchange, mockCreateApi);

      expect(result[1]).toBe(mockAxiosInstance);
    });

    it('should return secondaryXhr as third element', () => {
      const result = createApiWithSecondaryKey(optionsWithSecondary, mockExchange, mockCreateApi);

      expect(result[2]).toBe(mockSecondaryAxiosInstance);
    });
  });

  describe('edge cases', () => {
    it('should handle empty secondary_key value', () => {
      const options: ExchangeOptions = {
        key: 'primary-key',
        secret: 'primary-secret',
        secondary_key: '',
      };

      // Empty string is falsy, so should behave like no secondary key
      const result = createApiWithSecondaryKey(options, mockExchange, mockCreateApi);

      expect(result).toHaveLength(3);
      expect(mockCreateApi).toHaveBeenCalledTimes(1);
    });

    it('should handle undefined otherOptions', () => {
      const options: ExchangeOptions = {
        key: 'primary-key',
        secret: 'primary-secret',
      };

      const result = createApiWithSecondaryKey(options, mockExchange, mockCreateApi);

      expect(result).toHaveLength(3);
    });

    it('should handle options with only some secondary fields', () => {
      const options: ExchangeOptions = {
        key: 'primary-key',
        secret: 'primary-secret',
        secondary_key: 'secondary-key',
        // No secondary_secret
      };

      createApiWithSecondaryKey(options, mockExchange, mockCreateApi);

      const secondCallOptions = mockCreateApi.mock.calls[1][0];
      expect(secondCallOptions.key).toBe('secondary-key');
      // Secret should still be the original
      expect(secondCallOptions.secret).toBe('primary-secret');
    });
  });
});
