import type { ExchangeOptions } from '../../../types';

import { AxiosHeaders } from 'axios';
import retry from 'axios-retry';
import createHmac from 'create-hmac';
import qs from 'qs';

import { uuid } from '@tealstreet/safe-cex/src/utils/uuid';
import { safeCexAxiosCreate } from '../../../safeCexAxiosCreate';
import { virtualClock } from '../../../utils/virtualClock';
import { networkErrorOnRead } from '../../../utils/retryReadsOnly';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL } from '../../../exchanges/blofin/blofin.types';

// 152401 = access key does not exist, 152406 = IP not in the key's whitelist.
// Both are configuration faults that retrying cannot fix.
const BLOFIN_AUTH_ERROR_CODES = new Set(['152401', '152406']);

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const { passphrase } = options;

  if (!passphrase && !options.publicDataOnly) {
    throw new Error('Blofin requires a passphrase');
  }

  const baseURL = BASE_URL[options.testnet ? 'testnet' : 'livenet'];

  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      //timeout: options?.extra?.recvWindow ?? RECV_WINDOW,
      timeout: 20_000, // otherwise can't use leverage batch
    },
    options,
    exchange,
  );

  retry(xhr, { retries: 3, retryCondition: networkErrorOnRead() });

  // if (options.tealstreetTunnel) {
  //   xhr.defaults.adapter = axiosHttpOverWebsocketAdapter;
  // }

  xhr.interceptors.request.use((config) => {
    if (config?.url?.includes('public') || config?.url?.includes('market') || options.publicDataOnly) {
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
    // TODO: Audit whether HOW proxy re-serialization can break this signature (see bitmex.api.ts).
    const params = config.params ? decodeURIComponent(qs.stringify(config.params)) : null;
    const url = `${config.url}${params ? `?${params}` : ''}`;
    // A replayed config carries the body axios already serialized: sign those bytes.
    const data = typeof config.data === 'string' ? config.data : config.data ? JSON.stringify(config.data) : null;

    const timestamp = +virtualClock.getCurrentTime();
    const nonce = uuid();
    const toSign = [url, method, timestamp, nonce];

    if (data) {
      toSign.push(data);
    }

    const signature = btoa(createHmac('sha256', options.secret).update(toSign.join('')).digest('hex'));

    // const signature = this.binaryToBase64(
    //   this.encode(this.hmac(this.encode(auth), this.encode(this.secret), 'sha256', 'hex')),
    // );

    const headers = new AxiosHeaders({
      ...nextConfig.headers,
      'ACCESS-KEY': options.key,
      'ACCESS-SIGN': signature,
      'ACCESS-NONCE': nonce,
      'ACCESS-TIMESTAMP': timestamp,
      'ACCESS-PASSPHRASE': passphrase || '',
    });

    return { ...nextConfig, headers };
  });

  // Credential problems never resolve by retrying, so let the exchange back its
  // tick loop off instead of hammering a dead key at full rate forever. Only a
  // SIGNED success clears the streak — public endpoints keep returning code 0
  // with no credentials at all and would otherwise mask a rejected key.
  xhr.interceptors.response.use((response) => {
    if (!response.config?.headers?.['ACCESS-SIGN']) return response;

    const code = String(response.data?.code ?? '');
    if (BLOFIN_AUTH_ERROR_CODES.has(code)) {
      exchange.recordAuthFailure();
    } else if (code === '0') {
      exchange.recordAuthSuccess();
    }

    return response;
  });

  return xhr;
};
