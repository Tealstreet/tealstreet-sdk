import type { EngineHost, HostedSigningClock } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, RemoteSignerRequest, RemoteSignerResponse } from '../../types';

import type { InternalAxiosRequestConfig } from 'axios';

import axios, { AxiosHeaders } from 'axios';
import qs from 'qs';

import { markSkipRemoteSigning } from '../../remoteSigningAdapter';
import { signHttpCapture } from '../../utils/signHttpCapture';
import { uuid } from '../../utils/uuid';
import { createAsterDEXV1RequestInterceptor, isAsterDEXPublicEndpoint } from '../asterdex/asterdex.signing';
import { createBinanceRequestInterceptor, isBinancePublicSigningRequest } from '../binance/binance.signing';
import { finishBingxRequest, isBingxUnsignedRequest, prepareBingxRequest, serializeBingxParams } from '../bingx/bingx.signing';
import { finishBitgetRequest, prepareBitgetRequest } from '../bitget/bitget.signing';
import { BROKER_ID as BITGET_BROKER_ID } from '../bitget/bitget.types';
import { finishBitunixRequest, isBitunixUnsignedRequest, prepareBitunixRequest } from '../bitunix/bitunix.signing';
import { finishBlofinRequest, isBlofinUnsignedRequest, prepareBlofinRequest } from '../blofin/blofin.signing';
import { createBybitRequestInterceptor, isBybitUnsignedSigningRequest } from '../bybit/bybit.signing';
import { getBybitSiteHeaders, isBybitSiteIdExchange } from '../bybit/bybit.site';
import { BROKER_ID, RECV_WINDOW } from '../bybit/bybit.types';
import { finishOkxRequest, isOkxPublicSigningRequest, isOkxUnsignedSigningRequest, prepareOkxRequest } from '../okx/okx.signing';

const BINANCE = new Set<ExchangeName>(['binance', 'binancepm', 'binanceinverse', 'binancespot', 'binanceoptions']);
const BYBIT = new Set<ExchangeName>(['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader']);
const OKX = new Set<ExchangeName>(['okx', 'okxeu', 'okxdemo', 'okxeudemo']);
const BITGET = new Set<ExchangeName>(['bitget', 'bitgetusdc', 'bitgetinverse']);

/** Phemex's REST preimage `path ‖ query ‖ expiry ‖ body`, as the retired TS venue
 * built it: the decoded qs query without `?`, expiry in rounded seconds a minute on.
 */
export const preparePhemexRequest = (config: InternalAxiosRequestConfig, stampMs: number) => {
  const params = config.params ? decodeURIComponent(qs.stringify(config.params)) : '';
  // A replayed config carries the body axios already serialized: sign those bytes.
  const data = typeof config.data === 'string' ? config.data : config.data ? JSON.stringify(config.data) : '';
  const expiry = String(Math.round((stampMs + 60_000) / 1000));
  return { config, expiry, payload: `${config.url ?? ''}${params}${expiry}${data}` };
};

export const finishPhemexRequest = (
  prepared: ReturnType<typeof preparePhemexRequest>,
  key: string,
  signature: string,
): InternalAxiosRequestConfig => ({
  ...prepared.config,
  headers: new AxiosHeaders({
    ...prepared.config.headers,
    'Content-Type': 'application/json',
    'X-PHEMEX-ACCESS-TOKEN': key,
    'X-PHEMEX-REQUEST-EXPIRY': prepared.expiry,
    'X-PHEMEX-REQUEST-SIGNATURE': signature,
  }),
});

/** The surviving hosted wire adapter uses standalone codecs, never a TS venue.
 * It cannot fall back to options.secret: the callback always uses owned refs.
 */
export const signHostedHttp = async (
  name: ExchangeName,
  options: ExchangeOptions,
  primaryComplete: boolean,
  host: EngineHost,
  request: RemoteSignerRequest,
): Promise<RemoteSignerResponse> => {
  if (!request.config) return { id: request.id, error: 'Missing HTTP config for signing' };
  // Other dispatch families have no request codec here.
  const aster = name === 'asterdex';
  const okx = OKX.has(name);
  const bitget = BITGET.has(name);
  const bitunix = name === 'bitunix';
  const bingx = name === 'bingx';
  const phemex = name === 'phemex';
  const blofin = name === 'blofin';
  if (!BINANCE.has(name) && !BYBIT.has(name) && !okx && !bitget && !bitunix && !bingx && !phemex && !blofin && !aster)
    return { id: request.id, error: 'Hosted HTTP signing is not supported for this venue yet' };
  const binance = BINANCE.has(name);
  // The legacy Bitget and Phemex interceptors sign every request once credentials exist.
  const unsigned = phemex ? false : binance ? isBinancePublicSigningRequest(name, request.config.url)
    : okx ? isOkxUnsignedSigningRequest(request.config.url)
    : aster ? isAsterDEXPublicEndpoint(request.config.url)
      : bitunix ? isBitunixUnsignedRequest(request.config.url)
      : bingx ? isBingxUnsignedRequest(request.config.url, options.testnet)
      : blofin ? isBlofinUnsignedRequest(request.config.url)
      : !bitget && isBybitUnsignedSigningRequest(request.config.method?.toLowerCase() ?? 'get', request.config.url);
  const needsCredential = primaryComplete && !unsigned;
  const credential = needsCredential ? (await host.hostedSigning({ kind: 'credential' })).credential : undefined;
  if (needsCredential && !credential) throw new Error('Signing credential is unavailable');
  let clock: HostedSigningClock | undefined;
  const now = () => {
    clock = host.hostedSigningClock();
    return clock.stampMs;
  };
  const signingOptions: ExchangeOptions = {
    ...options,
    key: credential ?? '',
    secret: '',
    remoteSigner: undefined,
    payloadSigner: async ({ payload }) => {
      if (!clock) throw new Error('Signing clock is unavailable');
      const output = await host.hostedSigning({ kind: 'http', payload, ...clock });
      // The exact preimage already contains this API key for Bybit. Never
      // return a MAC under changed provider material with an unrelated header.
      if (output.credential !== credential || !output.signature) throw new Error('Signing identity changed');
      return { signature: output.signature, ...(output.headers === undefined ? {} : { headers: Object.fromEntries(output.headers) }) };
    },
  };
  const siteHeaders = isBybitSiteIdExchange(name) ? getBybitSiteHeaders(options) : {};
  if (aster) {
    // AsterDEX V1 only: the host admits no V3 wallet account to hosted signing.
    const xhr = axios.create({ timeout: 10_000, headers: { 'Content-Type': 'application/json' } });
    xhr.interceptors.request.use(createAsterDEXV1RequestInterceptor(signingOptions, now));
    return signHttpCapture(xhr, request);
  }
  const xhr = axios.create({
    timeout: 20_000,
    paramsSerializer: {
      serialize: (params) => binance ? qs.stringify(params, { arrayFormat: 'repeat' })
        : bingx ? serializeBingxParams(params) : qs.stringify(params),
    },
    headers: okx ? {
      'Content-Type': 'application/json',
      ...(options.testnet || name.endsWith('demo') ? { 'x-simulated-trading': 1 } : {}),
    } : bitget ? {
      'X-CHANNEL-API-CODE': BITGET_BROKER_ID,
      'Content-Type': 'application/json',
    } : bitunix ? {
      ...(options.testnet ? { 'x-simulated-trading': 1 } : {}),
    } : bingx ? {
      'X-BX-APIKEY': credential,
      'X-SOURCE-KEY': 'Tealstreet',
      'Content-Type': 'application/json, charset=utf-8',
    } : phemex ? {} : blofin ? {
      'Content-Type': 'application/json',
    } : binance ? {
      'X-MBX-APIKEY': credential,
      'Content-Type': 'application/json, charset=utf-8',
    } : {
      'X-Referer': BROKER_ID,
      'X-BAPI-RECV-WINDOW': options.extra?.recvWindow ?? RECV_WINDOW,
      'Content-Type': 'application/json, charset=utf-8',
      ...siteHeaders,
    },
  });
  xhr.interceptors.request.use(okx ? async (config) => {
    // Market paths keep the existing local signing behavior. The skip flag
    // concerns the keyless transport roundtrip, not browser-held signing.
    if (isOkxPublicSigningRequest(config.url)) {
      markSkipRemoteSigning(config);
    }
    if (isOkxUnsignedSigningRequest(config.url) || !credential) return config;
    const prepared = prepareOkxRequest(config, new Date(now()).toISOString());
    const output = await host.hostedSigning({ kind: 'http', payload: prepared.payload, ...clock! });
    const passphrase = output.headers?.find(([key]) => key.toLowerCase() === 'ok-access-passphrase')?.[1];
    if (output.credential !== credential || !output.signature || !passphrase) throw new Error('Signing identity changed');
    return finishOkxRequest(prepared, credential, output.signature, passphrase);
  } : bitget ? async (config) => {
    if (!credential) return config;
    const prepared = prepareBitgetRequest(config, now());
    const output = await host.hostedSigning({ kind: 'http', payload: prepared.payload, ...clock! });
    const passphrase = output.headers?.find(([key]) => key.toLowerCase() === 'access-passphrase')?.[1];
    if (output.credential !== credential || !output.signature || !passphrase) throw new Error('Signing identity changed');
    return finishBitgetRequest(prepared, credential, output.signature, passphrase);
  } : bitunix ? async (config) => {
    if (isBitunixUnsignedRequest(config.url) || !credential) return config;
    // The preimage carries the presented key, so the engine signs exactly it.
    const prepared = prepareBitunixRequest(config, credential, now(), uuid());
    const output = await host.hostedSigning({ kind: 'http', payload: prepared.payload, ...clock! });
    if (output.credential !== credential || !output.signature) throw new Error('Signing identity changed');
    return finishBitunixRequest(prepared, credential, output.signature);
  } : phemex ? async (config) => {
    if (!credential) return config;
    const prepared = preparePhemexRequest(config, now());
    const output = await host.hostedSigning({ kind: 'http', payload: prepared.payload, ...clock! });
    if (output.credential !== credential || !output.signature) throw new Error('Signing identity changed');
    return finishPhemexRequest(prepared, credential, output.signature);
  } : bingx ? async (config) => {
    if (isBingxUnsignedRequest(config.url, options.testnet) || !credential) return config;
    const prepared = prepareBingxRequest(config, now());
    const output = await host.hostedSigning({ kind: 'http', payload: prepared.payload, ...clock! });
    if (output.credential !== credential || !output.signature) throw new Error('Signing identity changed');
    return finishBingxRequest(prepared, credential, output.signature);
  } : blofin ? async (config) => {
    if (isBlofinUnsignedRequest(config.url) || !credential) return config;
    const prepared = prepareBlofinRequest(config, now(), uuid());
    const output = await host.hostedSigning({ kind: 'http', payload: prepared.payload, ...clock! });
    const passphrase = output.headers?.find(([key]) => key.toLowerCase() === 'access-passphrase')?.[1];
    if (output.credential !== credential || !output.signature || !passphrase) throw new Error('Signing identity changed');
    return finishBlofinRequest(prepared, credential, output.signature, passphrase);
  } : binance
    ? createBinanceRequestInterceptor(signingOptions, name, now)
    : createBybitRequestInterceptor(signingOptions, name, now, siteHeaders));
  return signHttpCapture(xhr, request);
};

export const signHostedWsAuth = async (
  host: EngineHost,
  request: RemoteSignerRequest,
): Promise<RemoteSignerResponse> => {
  const { toSign, outputFormat } = request.wsAuth ?? {};
  if (!toSign || !outputFormat) return { id: request.id, error: 'Missing wsAuth payload for WS auth signing' };
  if (outputFormat === 'hyperliquid-l1' || outputFormat === 'hyperliquid-address' || outputFormat === 'lighter-tx'
    || outputFormat === 'paradex-stark' || outputFormat === 'paradex-public')
    return { id: request.id, wsAuthSignature: host.hostedWalletSigning({ toSign, outputFormat }) };
  if (outputFormat !== 'hex' && outputFormat !== 'base64' && outputFormat !== 'hex-base64'
    && outputFormat !== 'bitunix-double-sha256')
    return { id: request.id, error: 'Unsupported WS auth output format' };
  const output = await host.hostedSigning({ kind: 'wsAuth', toSign, outputFormat });
  if (!output.signature) throw new Error('Signing result is unavailable');
  return { id: request.id, wsAuthSignature: output.signature };
};
