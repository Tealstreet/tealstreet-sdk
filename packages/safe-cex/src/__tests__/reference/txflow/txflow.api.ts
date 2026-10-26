import type { AxiosInstance } from 'axios';
import type { ExchangeOptions } from '../../../types';
import type { BaseExchange } from '../../../exchanges/base';

import { markSkipRemoteSigning } from '../../../remoteSigningAdapter';
import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import { getTransport } from '../../../transport/registry';
import { TXFLOW_API_VERSION, TXFLOW_TESTNET_REST_URL } from './txflow.types';

export { TXFLOW_MAINNET_REST_URL, TXFLOW_TESTNET_REST_URL } from './txflow.types';

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange): AxiosInstance => {
  const xhr = safeCexAxiosCreate(
    { baseURL: TXFLOW_TESTNET_REST_URL, headers: { 'Content-Type': 'application/json', 'x-api-version': TXFLOW_API_VERSION } },
    options,
    exchange,
  );
  installTransportAdapter(xhr, options, exchange.name);
  xhr.interceptors.request.use((config) => {
    if (config.url === '/info') markSkipRemoteSigning(config);
    return config;
  });
  return xhr;
};

export const createPublicAPI = (options: ExchangeOptions, exchange: BaseExchange): AxiosInstance =>
  createAPI({
    ...options,
    publicDataOnly: true,
    key: '',
    secret: '',
    remoteSigner: undefined,
    tealstreetTunnel: false,
    overrideProxyUrl: getTransport(options).publicCorsProxy(),
  }, exchange);
