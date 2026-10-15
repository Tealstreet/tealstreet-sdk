import type { ExchangeOptions } from '../../types';
import type { IsSignatureRejection } from '../../utils/signatureRejection';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { markSkipRemoteSigning } from '../../remoteSigningAdapter';
import { safeCexAxiosCreate, installTransportAdapter } from '../../safeCexAxiosCreate';
import { virtualClock } from '../../utils/virtualClock';
import { networkErrorOnRead } from '../../utils/retryReadsOnly';
import { resendOnSignatureRejection, signatureRejectedResponse } from '../../utils/signatureRejection';
import { BaseExchange } from '../base';
import type { OKXExchange } from './okx.exchange';
import { BASE_URLS, RECV_WINDOW } from './okx.types';

// Broader than the signing check below, which matches only `/api/v5/public/`
// and so misses tickers and both candle endpoints under `/api/v5/market/`.
// Used only to skip the hosted signing round trip, never to decide signing.
const isPublicEndpoint = (url?: string) => Boolean(url && (url.includes('/public/') || url.includes('/market/')));

export const isOkxSignatureRejection: IsSignatureRejection = (data) =>
  String(data?.code) === '50113' || /invalid sign/i.test(String(data?.msg ?? ''));

const formatOkxApiErrorMessage = (data: any): string => {
  const firstFailedRow = Array.isArray(data?.data)
    ? data.data.find((row: any) => row && ((row.sCode && row.sCode !== '0') || row.sMsg))
    : undefined;
  const rowMessage = typeof firstFailedRow?.sMsg === 'string' ? firstFailedRow.sMsg.trim() : '';
  const rowCode = firstFailedRow?.sCode === undefined ? '' : String(firstFailedRow.sCode);

  if (rowMessage) {
    return rowCode && rowCode !== '0' ? `${rowMessage} (code ${rowCode})` : rowMessage;
  }

  const message = typeof data?.msg === 'string' ? data.msg.trim() : '';
  if (message) return message;

  const code = data?.code === undefined ? 'unknown' : String(data.code);
  return `OKX API error (code ${code})`;
};

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const { passphrase } = options;

  if (!passphrase && !options.publicDataOnly) {
    throw new Error('OKX requires a passphrase');
  }

  const region = (exchange as OKXExchange).region ?? 'okx';
  const baseURL = BASE_URLS[region][options.testnet ? 'testnet' : 'livenet'];

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      headers: {
        'Content-Type': 'application/json',
        ...(options.testnet ? { 'x-simulated-trading': 1 } : {}),
      },
    },
    options,
    exchange,
  );

  retry(xhr, {
    retries: 3,
    retryCondition: (error) => networkErrorOnRead()(error) || signatureRejectedResponse(isOkxSignatureRejection)(error),
  });

  // Check OKX API-level errors (non-zero code) and convert to thrown errors
  // so they surface properly instead of crashing on missing `data` field
  xhr.interceptors.response.use((response) => {
    const signatureResend = resendOnSignatureRejection(xhr, response, isOkxSignatureRejection);
    if (signatureResend) return signatureResend;

    const code = response.data?.code;
    if (code && code !== '0') {
      const msg = formatOkxApiErrorMessage(response.data);
      const err: any = new Error(msg);
      err.response = response;
      throw err;
    }
    return response;
  });

  xhr.interceptors.request.use((config) => {
    // Tag first, then fall through: whether this gets signed locally is decided
    // by the branches below, exactly as before.
    if (isPublicEndpoint(config.url)) markSkipRemoteSigning(config);

    if (config?.url?.includes('public')) {
      return config;
    }

    const nextConfig = { ...config };

    if (options.remoteSigner) {
      return nextConfig;
    }

    // Skip signing when no API key (public-only mode)
    if (!options.key || !options.secret) {
      return nextConfig;
    }

    const method = config.method?.toUpperCase?.() || 'GET';
    const params = config.params ? decodeURIComponent(qs.stringify(config.params)) : null;
    const url = `${config.url}${params ? `?${params}` : ''}`;
    const hasBody = config.data !== undefined && config.data !== null;
    const data = hasBody ? (typeof config.data === 'string' ? config.data : JSON.stringify(config.data) || '') : '';

    const timestamp = virtualClock.getCurrentTime().toISOString();
    const toSign = [timestamp, method, url, data];

    const signature = createHmac('sha256', options.secret).update(toSign.join('')).digest('base64');

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'OK-ACCESS-KEY': options.key,
      'OK-ACCESS-SIGN': signature,
      'OK-ACCESS-TIMESTAMP': timestamp,
      'OK-ACCESS-PASSPHRASE': passphrase || '',
    });

    return { ...nextConfig, ...(hasBody ? { data } : {}), headers };
  });

  if (options.tealstreetTunnel) {
    installTransportAdapter(xhr, options, exchange.name, 'bigInt');
  }

  return xhr;
};
