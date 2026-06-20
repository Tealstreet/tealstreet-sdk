import type { BaseExchange } from './exchanges/base';

import { AxiosInstance, AxiosRequestConfig } from 'axios';

import { ExchangeOptions, Writable } from './types';

export const createApiWithSecondaryKey = (
  options: ExchangeOptions,
  exchange: BaseExchange,
  createApi: (options: ExchangeOptions, exchange: BaseExchange) => AxiosInstance,
  otherOptions: {
    proxyUrl?: string;
    rateLimit?: (axiosInstance: AxiosInstance) => AxiosInstance;
    secondaryRateLimit?: (axiosInstance: AxiosInstance) => AxiosInstance;
  } = {},
) => {
  const proxyUrl =
    options.secondaryProxyUrl || otherOptions.proxyUrl || 'https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy';

  let defaultXhr = createApi(options, exchange);
  if (otherOptions.rateLimit) {
    defaultXhr = otherOptions.rateLimit(defaultXhr);
  }
  if (!options.secondary_key) {
    return [defaultXhr, defaultXhr, defaultXhr];
  }
  const secondaryOptionOverrides = Object.keys(options).reduce((acc, key) => {
    if (key.startsWith('secondary_')) {
      const newKey = key.replace('secondary_', '') as any as keyof ExchangeOptions;
      acc[newKey] = options[key as keyof ExchangeOptions];
    }
    return acc;
  }, {} as any);

  const secondaryOptions: Writable<ExchangeOptions> = {
    ...options,
    ...secondaryOptionOverrides,
    overrideProxyUrl: proxyUrl,
  };

  let secondaryXhr = createApi(secondaryOptions, exchange);
  if (otherOptions.secondaryRateLimit) {
    secondaryXhr = otherOptions.secondaryRateLimit(secondaryXhr);
  }

  const dynamicXhr = new Proxy(defaultXhr, {
    get: function (target, prop, receiver) {
      if (prop === 'get') {
        return function (url: string, config?: AxiosRequestConfig) {
          return secondaryXhr.get(url, config);
        };
      }
      return Reflect.get(target, prop, receiver);
    },
  });

  return [dynamicXhr, defaultXhr, secondaryXhr];
};
