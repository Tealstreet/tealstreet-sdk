import type { AxiosAdapter, AxiosInstance, InternalAxiosRequestConfig } from 'axios';

import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

import { AxiosError } from 'axios';
import { describe, expect, it, vi } from 'vitest';

import { createAPI as createApexAPI } from '../reference/apexomni/apexomni.api';
import { createAPI as createAsterdexAPI } from '../../exchanges/asterdex/asterdex.api';
import { createAPI as createBackpackAPI } from '../reference/backpack/backpack.api';
import { createAPI as createBinanceAPI } from '../reference/binance/binance.api';
import { createPAPI } from '../reference/binance/binancepm.api';
import { createAPI as createBinanceSpotAPI } from '../reference/binance/binancespot.api';
import { createAPI as createBingxAPI } from '../reference/bingx/bingx.api';
import { ENDPOINTS as BINGX } from '../reference/bingx/bingx.types';
import { createAPI as createBitgetAPI } from '../reference/bitget/bitget.api';
import { createAPI as createBitrueAPI } from '../reference/bitrue/bitrue.api';
import { createAPI as createBitunixAPI } from '../reference/bitunix/bitunix.api';
import { createAPI as createBlofinAPI } from '../reference/blofin/blofin.api';
import { createAPI as createBreakoutAPI } from '../reference/breakoutprop/breakoutprop.api';
import { createAPI as createBybitAPI } from '../reference/bybit/bybit.api';
import { createAPI as createCoinbaseAPI } from '../reference/coinbase/coinbase.api';
import { createAPI as createGrvtAPI, createTradingAPI as createGrvtTradingAPI } from '../reference/grvt/grvt.api';
import { ENDPOINTS as GRVT } from '../../exchanges/grvt/grvt.types';
import { createExchangeAPI, createInfoAPI } from '../reference/hyperliquid/hyperliquid.api';
import { createAPI as createKrakenAPI } from '../reference/kraken/kraken.api';
import { createAPI as createKrakenSpotAPI } from '../reference/kraken/krakenspot.api';
import { ENDPOINTS as KRAKEN_SPOT } from '../reference/kraken/krakenspot.types';
import { createAPI as createKucoinAPI } from '../reference/kucoin/kucoin.api';
import { ENDPOINTS as KUCOIN } from '../reference/kucoin/kucoin.types';
import { createKucoinSpotAPI } from '../reference/kucoin/kucoinspot.api';
import { createAPI as createMexcAPI } from '../reference/mexc/mexc.api';
import { createAPI as createOkxAPI } from '../reference/okx/okx.api';
import { createAPI as createOrderlyAPI } from '../reference/orderly/orderly.api';
import { createAPI as createParadexAPI } from '../reference/paradex/paradex.api';
import { createAPI as createPhemexAPI } from '../reference/phemex/phemex.api';
import { createPolymarketAPI } from '../reference/polymarket/polymarket.api';
import { createAPI as createWooAPI } from '../reference/woo/woo.api';

// A connection reset after the venue accepted the request: the order is live,
// the response is lost. Resending it would place it twice.
const lostResponseAdapter = () => {
  const calls = vi.fn();
  const adapter: AxiosAdapter = (config) => {
    calls(config.url);
    return Promise.reject(new AxiosError('socket hang up', 'ECONNRESET', config as InternalAxiosRequestConfig, {}));
  };
  return { adapter, calls };
};

const send = async (xhr: AxiosInstance, [method, url]: Request) => {
  const { adapter, calls } = lostResponseAdapter();
  await xhr.request({ method, url, data: method === 'get' ? undefined : {}, adapter }).catch(() => undefined);
  return calls.mock.calls.length;
};

type Request = [method: string, url: string];
type Venue = { create: () => AxiosInstance; write: Request; read?: Request; readAttempts?: number };

const options = { key: 'k', secret: 's', passphrase: 'p', applicationId: 'a' } as any;
const publicOnly = { ...options, publicDataOnly: true };
// Just enough state for each venue's request interceptor to reach the transport.
const exchange = {
  name: 'test',
  options,
  log: vi.fn(),
  emit: vi.fn(),
  on: vi.fn(),
  emitAndLogError: vi.fn(),
  sessionToken: 'session',
  jwtManager: { getToken: async () => 'jwt' },
} as any;
const polymarketCredentials = () => ({ apiKey: 'k', secret: 'c2VjcmV0', passphrase: 'p' }) as any;
const polymarketWallet = () => '0x0000000000000000000000000000000000000001';

const venues: Record<string, Venue> = {
  okx: { create: () => createOkxAPI(options, exchange), write: ['post', '/api/v5/trade/order'], read: ['get', '/a'] },
  blofin: {
    create: () => createBlofinAPI(options, exchange),
    write: ['post', '/api/v1/trade/order'],
    read: ['get', '/a'],
  },
  mexc: { create: () => createMexcAPI(options, exchange), write: ['post', '/order/submit'], read: ['get', '/a'] },
  coinbase: { create: () => createCoinbaseAPI(publicOnly, exchange), write: ['post', '/orders'], read: ['get', '/a'] },
  hyperliquid: {
    create: () => createExchangeAPI(publicOnly, exchange),
    write: ['post', '/exchange'],
    read: ['post', '/info'],
  },
  'hyperliquid info': {
    create: () => createInfoAPI(publicOnly, exchange),
    write: ['post', '/exchange'],
    read: ['post', '/info'],
  },
  grvt: {
    create: () => createGrvtAPI(options, exchange),
    write: ['post', GRVT.CREATE_ORDER],
    read: ['post', GRVT.POSITIONS],
    readAttempts: 6,
  },
  'grvt trading': {
    create: () => createGrvtTradingAPI(options, exchange),
    write: ['post', '/full/v1/create_order'],
    read: ['post', '/full/v1/open_orders'],
    readAttempts: 6,
  },
  binance: {
    create: () => createBinanceAPI(publicOnly, exchange),
    write: ['post', '/fapi/v1/order'],
    read: ['get', '/a'],
  },
  'binance cancel': { create: () => createBinanceAPI(publicOnly, exchange), write: ['delete', '/fapi/v1/order'] },
  'binance listen key': {
    create: () => createBinanceAPI(publicOnly, exchange),
    write: ['post', '/fapi/v1/order'],
    read: ['post', '/fapi/v1/listenKey'],
  },
  binancepm: {
    create: () => createPAPI(publicOnly, exchange),
    write: ['post', '/papi/v1/um/order'],
    read: ['get', '/a'],
  },
  binancespot: {
    create: () => createBinanceSpotAPI(publicOnly, exchange),
    write: ['post', '/api/v3/order'],
    read: ['get', '/a'],
  },
  bitget: {
    create: () => createBitgetAPI(publicOnly, exchange),
    write: ['post', '/api/v2/mix/order/place-order'],
    read: ['get', '/a'],
  },
  woo: { create: () => createWooAPI(publicOnly, exchange), write: ['post', '/v1/order'], read: ['get', '/a'] },
  // `/public/` skips ed25519 signing; the retry rule depends on the method alone.
  orderly: { create: () => createOrderlyAPI(publicOnly, exchange), write: ['post', '/v1/public/order'] },
  'orderly amend': { create: () => createOrderlyAPI(publicOnly, exchange), write: ['put', '/v1/public/order'] },
  phemex: {
    create: () => createPhemexAPI(publicOnly, exchange),
    write: ['put', '/g-orders/replace'],
    read: ['get', '/a'],
  },
  // Without a key the ed25519 signer is skipped; the retry rule depends on the method alone.
  backpack: {
    create: () => createBackpackAPI({} as any, exchange),
    write: ['post', '/api/v1/order'],
    read: ['get', '/a'],
  },
  bitunix: {
    create: () => createBitunixAPI(publicOnly, exchange),
    write: ['post', '/trade/place_order'],
    read: ['get', '/a'],
  },
  kraken: {
    create: () => createKrakenAPI(publicOnly, exchange),
    write: ['post', '/v3/sendorder'],
    read: ['get', '/a'],
  },
  krakenspot: {
    create: () => createKrakenSpotAPI(publicOnly, exchange),
    write: ['post', KRAKEN_SPOT.ADD_ORDER],
    read: ['post', KRAKEN_SPOT.OPEN_ORDERS],
  },
  bingx: {
    create: () => createBingxAPI({} as any, exchange),
    write: ['post', '/trade/order'],
    read: ['post', BINGX.LISTEN_KEY],
  },
  kucoin: {
    create: () => createKucoinAPI(publicOnly, exchange),
    write: ['post', '/api/v1/orders'],
    read: ['post', KUCOIN.GET_PRIVATE_WS_TOKEN],
  },
  kucoinspot: {
    create: () => createKucoinSpotAPI(publicOnly, exchange),
    write: ['post', '/api/v1/hf/orders'],
    read: ['get', '/a'],
  },
  breakoutprop: {
    create: () => createBreakoutAPI(publicOnly, exchange),
    write: ['post', '/orders'],
    read: ['get', '/a'],
  },
  paradex: { create: () => createParadexAPI(publicOnly, exchange), write: ['post', '/orders'] },
  apexomni: {
    create: () => createApexAPI(publicOnly, exchange),
    write: ['post', '/api/v3/order'],
    read: ['get', '/a'],
  },
  bitrue: {
    create: () => createBitrueAPI(publicOnly, exchange),
    write: ['post', '/fapi/v2/order'],
    read: ['get', '/a'],
  },
  bybit: {
    create: () => createBybitAPI(publicOnly, exchange),
    write: ['post', '/v5/order/create'],
    read: ['get', '/a'],
  },
  // Reads omitted where the retry delay is seconds or the interceptor needs real key material.
  polymarket: {
    create: () => createPolymarketAPI(publicOnly, exchange, polymarketCredentials, polymarketWallet),
    write: ['post', '/order'],
  },
  'polymarket cancel': {
    create: () => createPolymarketAPI(publicOnly, exchange, polymarketCredentials, polymarketWallet),
    write: ['delete', '/order'],
  },
  asterdex: {
    create: () => createAsterdexAPI({ ...exchange, options } as any, {}, options),
    write: ['post', '/fapi/v1/order'],
  },
  'asterdex cancel': {
    create: () => createAsterdexAPI({ ...exchange, options } as any, {}, options),
    write: ['delete', '/fapi/v1/order'],
  },
};

describe('audit regression: a write whose response was lost is never resent', () => {
  it.each(Object.entries(venues))('%s', async (_name, { create, write, read, readAttempts = 4 }) => {
    const xhr = create();
    expect(await send(xhr, write)).toBe(1);
    if (read) expect(await send(xhr, read)).toBe(readAttempts);
  });
});

// Catches a venue that gains an axios-retry without the read-only rule; the table above proves the rule.
describe('every axios-retry factory limits retries to reads', () => {
  const exchangesDir = path.resolve(__dirname, '../../exchanges');
  const ownRule = new Set(['weex/weex.api.ts', 'cryptocom/cryptocom.api.ts']);
  const factories = readdirSync(exchangesDir, { recursive: true, encoding: 'utf8' })
    .filter((file) => file.endsWith('.ts') && !file.endsWith('.test.ts'))
    .filter((file) => /from ['"]axios-retry['"]/.test(readFileSync(path.join(exchangesDir, file), 'utf8')));

  it.each(factories.filter((file) => !ownRule.has(file)))('%s', (file) => {
    const source = readFileSync(path.join(exchangesDir, file), 'utf8');
    expect(source).toContain('utils/retryReadsOnly');
    expect(source).not.toMatch(/retryCondition:\s*(isNetworkError|isNetworkOrIdempotentRequestError)\b/);
  });
});
