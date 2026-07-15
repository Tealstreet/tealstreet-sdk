import { AxiosHeaders } from 'axios';
import retry, { isNetworkError } from 'axios-retry';
import qs from 'qs';
import { axiosHttpOverWebsocketAdapter } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { safeCexAxiosCreate, setInnerAdapterForRemoteSigning } from '../../safeCexAxiosCreate';
import type { ExchangeOptions } from '../../types';
import { replaceUrlParams } from '../../utils/replaceUrlParams';
import type { BreakoutpropExchange } from './breakoutprop.exchange';
import { BASE_URL, RECV_WINDOW } from './breakoutprop.types';

const isErrorSignature = (error: any) => {
  const isNetwork = isNetworkError(error);
  const isErrSign =
    error?.response?.data?.ret_msg?.includes('error sign!') || error?.response?.data?.retMsg?.includes('error sign!');
  return isNetwork || isErrSign;
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
        // TODO: update referer
        // 'X-Referer': BROKER_ID,
        'Content-Type': 'application/json',
      },
    },
    options,
    exchange,
  );

  // retry requests on network errors instead of throwing
  // and some signature errors
  retry(xhr, {
    retries: 3,
    retryCondition: isErrorSignature,
  });

  if (options.tealstreetTunnel) {
    setInnerAdapterForRemoteSigning(xhr, axiosHttpOverWebsocketAdapter);
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

  return xhr;
};
