import type { ExchangeOptions } from '../../../types';
import type { IsSignatureRejection } from '../../../utils/signatureRejection';

import retry from 'axios-retry';
import createHmac from 'create-hmac';
import omit from 'lodash/omit';
import qs from 'qs';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { safeCexAxiosCreate, installTransportAdapter } from '../../../safeCexAxiosCreate';
import { tryParseJson } from '../../../utils/tryParseJson';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { BASE_URL, ENDPOINTS, RECV_WINDOW } from './bingx.types';
import { networkErrorOnRead, readsAndPosted } from '../../../utils/retryReadsOnly';
import { resendOnSignatureRejection, signatureRejectedResponse } from '../../../utils/signatureRejection';

const sort = function alphabeticalSort(a: string, b: string) {
  return a.localeCompare(b);
};

// A copy without the previous pass's signature. A replayed POST arrives with the body
// axios serialized, and a replayed GET with the signed params it sent.
const unsignedPayload = (payload: unknown): Record<string, any> => {
  const parsed = tryParseJson(payload);
  if (!parsed || typeof parsed !== 'object') return {};
  const { signature: _previous, ...rest } = parsed as Record<string, any>;
  return rest;
};

export const isBingxSignatureRejection: IsSignatureRejection = (data) =>
  Number(data?.code) === 100001 || /signature verification failed/i.test(String(data?.msg ?? ''));

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) => {
  const recvWindow = options?.extra?.recvWindow ?? RECV_WINDOW;
  const baseURL = BASE_URL[options.testnet ? 'testnet' : 'livenet'];
  const xhr = safeCexAxiosCreate(
    {
      baseURL,
      timeout: recvWindow,
      paramsSerializer: {
        serialize: (params) => qs.stringify(params, { arrayFormat: 'repeat', sort }),
      },
      headers: {
        'X-BX-APIKEY': options.key,
        'X-SOURCE-KEY': 'Tealstreet',
        'Content-Type': 'application/json, charset=utf-8',
      },
      // Bingx uses big ints (orderId in particular). We need to replace them with strings to avoid JS rounding errors
      transformResponse: bigIntAsStringJsonParse,
    },
    options,
    exchange,
  );

  // retry requests on network errors instead of throwing
  retry(xhr, {
    retries: 3,
    retryCondition: (error) =>
      networkErrorOnRead(readsAndPosted(ENDPOINTS.LISTEN_KEY))(error) ||
      signatureRejectedResponse(isBingxSignatureRejection)(error),
  });

  // Browser callers go through the HTTP-over-WebSocket tunnel by default
  // (BingX requires CORS + the tunnel hops through tealstreet's proxy IPs).
  // Headless callers (Node/Bun services) have direct egress and
  // opt out via `tealstreetTunnel: false`. Keeping the gate explicit so a
  // future audit doesn't reintroduce the unconditional adapter.
  if (!options.publicDataOnly && options.tealstreetTunnel !== false) {
    installTransportAdapter(xhr, options, exchange.name);
  }

  xhr.interceptors.request.use((config) => {
    // on livenet, don't sign listen key requests (they don't need it)
    if (config.url === ENDPOINTS.LISTEN_KEY && !options.testnet) {
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

    const timestamp = virtualClock.getCurrentTimeMs();

    // TODO: Audit whether HOW proxy re-serialization can break this signature (see bitmex.api.ts).
    if (config.method !== 'post') {
      const data = unsignedPayload(config.data || config.params);
      data.timestamp = timestamp;
      //data.recvWindow = recvWindow;

      // Create a new sorted data object for consistent field ordering
      const sortedData = Object.keys(data)
        .sort(sort)
        .reduce(
          (acc, key) => {
            acc[key] = data[key];
            return acc;
          },
          {} as Record<string, any>,
        );

      const asString = qs.stringify(sortedData, { arrayFormat: 'repeat', sort });
      const signature = createHmac('sha256', options.secret).update(asString).digest('hex');

      sortedData.signature = signature;
      nextConfig.params = sortedData;

      // remove data from POST/PUT/DELETE requests
      // Binance API takes data as query params
      return omit(nextConfig, 'data');
    } else {
      const data = unsignedPayload(config.data || config.params);
      data.timestamp = timestamp;
      //data.recvWindow = recvWindow;

      // Create a new sorted data object for consistent field ordering
      const sortedData = Object.keys(data)
        .sort(sort)
        .reduce(
          (acc, key) => {
            acc[key] = data[key];
            return acc;
          },
          {} as Record<string, any>,
        );

      const asString = qs.stringify(sortedData, { arrayFormat: 'repeat', sort });
      const signature = createHmac('sha256', options.secret).update(asString).digest('hex');

      sortedData.signature = signature;

      const fieldsToInts = ['quantity', 'price'];
      for (const fieldId of fieldsToInts) {
        if (sortedData[fieldId]) {
          sortedData[fieldId] = +sortedData[fieldId];
        }
      }

      nextConfig.headers['Content-Type'] = 'application/json';

      // remove data from POST/PUT/DELETE requests
      // Binance API takes data as query params
      // Serialized here: axios-retry resends with an identity transformRequest, which
      // would otherwise send this object unserialized.
      return { ...nextConfig, data: JSON.stringify(sortedData) };
    }
  });

  // BingX answers on HTTP 200, and over the tunnel any status resolves.
  xhr.interceptors.response.use(
    (response) => resendOnSignatureRejection(xhr, response, isBingxSignatureRejection) ?? response,
  );

  return xhr;
};

// {
//   "positionId": "1785561029271568384",
//   "symbol": "BTC-USDT",
//   "currency": "USDT",
//   "positionAmt": "0.0003",
//   "availableAmt": "0.0003",
//   "positionSide": "SHORT",
//   "isolated": false,
//   "avgPrice": "59739.3",
//   "initialMargin": "0.2240",
//   "margin": "0.2443",
//   "leverage": 80,
//   "unrealizedProfit": "0.0202",
//   "realisedProfit": "-0.0177",
//   "liquidationPrice": 873754.5192853138,
//   "pnlRatio": "0.0905",
//   "maxMarginReduction": "0.0184",
//   "riskRate": "0.0063",
//   "markPrice": "59671.7",
//   "positionValue": "17.9015",
//   "onlyOnePosition": true,
//   "updateTime": 1714546013983
// }

export type FETCH_POSITIONS_RESPONSE = {
  code: number;
  msg: string;
  data: {
    symbol: string;
    positionId: string;
    positionSide: string;
    isolated: boolean;
    positionAmt: string;
    availableAmt: string;
    unrealizedProfit: string;
    realisedProfit: string;
    initialMargin: string;
    Margin: string;
    avgPrice: string;
    liquidationPrice: number;
    leverage: number;
    positionValue: string;
    markPrice: string;
    riskRate: string;
    maxMarginReduction: string;
    pnlRatio: string;
    updateTime: number;
  }[];
};
