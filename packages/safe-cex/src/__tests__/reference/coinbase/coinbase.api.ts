import type { ExchangeOptions } from '../../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import qs from 'qs';

import { markSkipRemoteSigning } from '../../../remoteSigningAdapter';
import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import { getTransport } from '../../../transport/registry';
import { replaceUrlParams } from '../../../utils/replaceUrlParams';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL, PUBLIC_ENDPOINTS, RECV_WINDOW } from './coinbase.types';
import { sign } from './coinbase.utils';

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const xhr = safeCexAxiosCreate(
    {
      baseURL: BASE_URL[options.testnet ? 'testnet' : 'livenet'],
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { arrayFormat: 'repeat' }),
      },
      headers: {
        'X-MBX-APIKEY': options.key,
        'Content-Type': 'application/json, charset=utf-8',
      },
    },
    options,
    exchange,
  );

  // retry requests on network errors instead of throwing
  retry(xhr, { retries: 3, retryCondition: networkErrorOnRead() });

  xhr.interceptors.request.use(async (config) => {
    config = {
      ...config,
      ...replaceUrlParams(config.url, config.params),
    };

    const isPublicEndpointUrl = PUBLIC_ENDPOINTS.includes(`${config.url}`);
    if (isPublicEndpointUrl) markSkipRemoteSigning(config);

    const isPublicEndPoint = isPublicEndpointUrl || options.publicDataOnly;

    // proxy use with non base url
    if (config?.url?.startsWith('http')) {
      const privateProxyUrl = getTransport(options).proxyPrefix({ exchange: 'coinbase', options, config });
      const isPrivateProxyEnabled = privateProxyUrl !== '';
      config.url = isPrivateProxyEnabled ? `${privateProxyUrl}/${config.url}` : config.url;
    }

    if (isPublicEndPoint) {
      return {
        ...config,
        headers: AxiosHeaders.from({
          'Content-Type': 'application/json, charset=utf-8',
        }),
        timeout: 20_000,
      };
    }

    if (options.remoteSigner) {
      return { ...config, timeout: 20_000 };
    }

    const request_method = config.method?.toUpperCase() || 'GET';
    const baseUrl = 'api.coinbase.com';
    const request_path = config.url;
    const uri = request_method + ' ' + baseUrl + request_path;

    const token = await sign(options.key, options.secret, uri);

    const headers = new AxiosHeaders({
      ...config.headers,
      ...(options.tealstreetTunnel
        ? { Authorization: `Bearer ${token}` }
        : { 'Coinbase-Authorization': `Bearer ${token}` }),
    });

    return { ...config, headers, timeout: 20_000 };
  });

  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  return xhr;
};
