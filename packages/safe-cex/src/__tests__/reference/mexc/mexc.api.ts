import type { AxiosRequestConfig } from 'axios';
import type { ExchangeOptions } from '../../../types';
import type { IsSignatureRejection } from '../../../utils/signatureRejection';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { markSkipRemoteSigning } from '../../../remoteSigningAdapter';
import { safeCexAxiosCreate } from '../../../safeCexAxiosCreate';
import { virtualClock } from '../../../utils/virtualClock';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';
import { resendOnSignatureRejection, signatureRejectedResponse } from '../../../utils/signatureRejection';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL, PUBLIC_ENDPOINTS, RECV_WINDOW } from './mexc.types';

export const serializeMexcParams = (params: unknown): string =>
  qs.stringify(params, {
    arrayFormat: 'repeat',
    sort: (a, b) => a.localeCompare(b),
  });

export const signMexcRequest = (secret: string, apiKey: string, requestTime: string, requestParam: string): string =>
  createHmac('sha256', secret)
    .update(`${apiKey}${requestTime}${requestParam}`)
    .digest('hex');

export const signMexcWsLogin = (secret: string, apiKey: string, reqTime: string): string =>
  createHmac('sha256', secret)
    .update(`${apiKey}${reqTime}`)
    .digest('hex');

export const isMexcPublicEndpoint = (url?: string): boolean => {
  if (!url) return false;
  const path = url.split('?')[0];
  return PUBLIC_ENDPOINTS.some((endpoint) => path === endpoint || path.startsWith(`${endpoint}/`));
};

const normalizeRequestData = (data: unknown): string => {
  if (data === undefined || data === null || data === '') return '';
  return typeof data === 'string' ? data : JSON.stringify(data);
};

const getQueryString = (config: AxiosRequestConfig): string => {
  if (config.params) return serializeMexcParams(config.params);

  const [, query = ''] = String(config.url || '').split('?');
  return query;
};

export const isMexcSignatureRejection: IsSignatureRejection = (data) =>
  Number(data?.code) === 602 || /signature verification failed/i.test(String(data?.message ?? data?.msg ?? ''));

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const xhr = safeCexAxiosCreate(
    {
      baseURL: BASE_URL[options.testnet ? 'testnet' : 'livenet'],
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      paramsSerializer: {
        serialize: serializeMexcParams,
      },
      headers: {
        'Content-Type': 'application/json',
      },
      transformResponse: bigIntAsStringJsonParse,
    },
    options,
    exchange,
  );

  retry(xhr, {
    retries: 3,
    retryCondition: (error) => networkErrorOnRead()(error) || signatureRejectedResponse(isMexcSignatureRejection)(error),
  });

  xhr.interceptors.request.use((config) => {
    const nextConfig = { ...config };
    if (isMexcPublicEndpoint(nextConfig.url)) {
      markSkipRemoteSigning(nextConfig);
      return nextConfig;
    }

    if (options.publicDataOnly) {
      return nextConfig;
    }

    if (options.remoteSigner || !options.key || !options.secret) {
      return nextConfig;
    }

    const method = nextConfig.method?.toUpperCase?.() || 'GET';
    const requestTime = `${virtualClock.getCurrentTimeMs()}`;
    const bodyString = method === 'GET' ? '' : normalizeRequestData(nextConfig.data);
    const requestParam = method === 'GET' ? getQueryString(nextConfig) : bodyString;
    const signature = signMexcRequest(options.secret, options.key, requestTime, requestParam);

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      ApiKey: options.key,
      'Request-Time': requestTime,
      Signature: signature,
      'Recv-Window': `${options?.extra?.recvWindow ?? RECV_WINDOW}`,
      'Content-Type': 'application/json',
    });

    return {
      ...nextConfig,
      data: bodyString || nextConfig.data,
      headers,
    };
  });

  xhr.interceptors.response.use((response) => {
    const signatureResend = resendOnSignatureRejection(xhr, response, isMexcSignatureRejection);
    if (signatureResend) return signatureResend;

    const data = response.data;
    if (data && typeof data === 'object') {
      const code = data.code;
      const success = data.success;
      const message = data.message || data.msg;
      if ((success === false || (code !== undefined && `${code}` !== '0' && `${code}` !== '200')) && message) {
        const err: any = new Error(message);
        err.response = response;
        throw err;
      }
    }

    return response;
  });

  return xhr;
};
