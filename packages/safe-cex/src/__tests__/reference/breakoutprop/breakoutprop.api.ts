import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import qs from 'qs';
import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import type { ExchangeOptions } from '../../../types';
import { replaceUrlParams } from '../../../utils/replaceUrlParams';
import type { BreakoutpropExchange } from './breakoutprop.exchange';
import { BASE_URL, ENDPOINTS, RECV_WINDOW } from './breakoutprop.types';
import { networkErrorOnRead, readsAndPosted } from '../../../utils/retryReadsOnly';

type SessionRetryConfig = {
  url?: string;
  __dxSessionRetried?: boolean;
};

export const createAPI = (options: ExchangeOptions, exchange: BreakoutpropExchange) => {
  const recvWindow = options?.extra?.recvWindow ?? RECV_WINDOW;
  const baseURL = BASE_URL;
  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: recvWindow,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params),
      },
      headers: {
        'Content-Type': 'application/json',
      },
    },
    options,
    exchange,
  );

  // DXtrade session auth has no request signatures. Retry transport failures;
  // the response interceptor below handles one explicit 401 re-login.
  retry(xhr, {
    retries: 3,
    retryCondition: networkErrorOnRead(readsAndPosted(ENDPOINTS.LOGIN, ENDPOINTS.MARKET_DATA)),
  });

  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  xhr.interceptors.request.use((config) => {
    if (config.url !== '/login' && !exchange.sessionToken) {
      throw new Error('No session token');
    }

    const nextConfig = {
      ...config,
      ...replaceUrlParams(config.url, config.params),
    };

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      ...(options.tealstreetTunnel
        ? { Authorization: `DXAPI ${exchange.sessionToken}` }
        : { 'Coinbase-Authorization': `DXAPI ${exchange.sessionToken}` }),
    });

    return { ...nextConfig, headers };
  });

  xhr.interceptors.response.use(undefined, async (error) => {
    const config = error?.config as SessionRetryConfig | undefined;
    if (
      error?.response?.status !== 401 ||
      !config ||
      config.url === '/login' ||
      config.__dxSessionRetried
    ) {
      throw error;
    }

    config.__dxSessionRetried = true;
    exchange.invalidateSession();
    await exchange.getSessionToken();
    return xhr.request(config);
  });

  return xhr;
};
