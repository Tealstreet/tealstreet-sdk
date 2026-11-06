import type { AxiosInstance, InternalAxiosRequestConfig } from 'axios';

import { AxiosError, AxiosHeaders } from 'axios';
import createHmac from 'create-hmac';
import dayjs from 'dayjs';
import { ethers } from 'ethers';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createAPI as createAsterdexAPI } from '../exchanges/asterdex/asterdex.api';
import { asterDEXVirtualClock } from '../exchanges/asterdex/asterdex.virtualClock';
import { createAPI as createBinanceAPI } from './reference/binance/binance.api';
import { createBinanceAPI as createBinancePmAPI } from './reference/binance/binancepm.api';
import { createAPI as createBinanceSpotAPI } from './reference/binance/binancespot.api';
import { createAPI as createBingxAPI } from './reference/bingx/bingx.api';
import { createAPI as createBitgetAPI } from './reference/bitget/bitget.api';
import { createAPI as createBitunixAPI } from './reference/bitunix/bitunix.api';
import { createAPI as createCryptocomAPI } from './reference/cryptocom/cryptocom.api';
import { createAPI as createKucoinAPI } from './reference/kucoin/kucoin.api';
import { createKucoinSpotAPI } from './reference/kucoin/kucoinspot.api';
import { createAPI as createKrakenSpotAPI } from './reference/kraken/krakenspot.api';
import { createAPI as createMexcAPI } from './reference/mexc/mexc.api';
import { createAPI as createOkxAPI } from './reference/okx/okx.api';
import { virtualClock } from '../utils/virtualClock';

// A signature rejection executed nothing, so each venue re-signs and resends once —
// whether it arrives as a rejected 4xx or resolved (a 200 envelope, or any status
// over the HOW tunnel) — and a second rejection surfaces.

const exchange = (name: string) =>
  ({
    name,
    emitter: { emit: () => undefined },
    emitAndLogError: () => undefined,
    recordAuthFailure: () => undefined,
    recordAuthSuccess: () => undefined,
    grafanaEndpointsEvery: [],
    grafanaEndpointsPeriodic: [],
  }) as any;

const keys = { key: 'api-key', secret: 'api-secret', passphrase: 'pass', tealstreetTunnel: false } as any;

type Venue = {
  name: string;
  create: () => AxiosInstance;
  write: { method: 'post'; url: string; data: Record<string, unknown> };
  rejection: Record<string, unknown>;
  ok: Record<string, unknown>;
  /** What changes when the request is signed again. */
  signatureOf: (config: InternalAxiosRequestConfig) => unknown;
};

const binanceStyle = { code: -1022, msg: 'Signature for this request is not valid.' };
const binanceSignature = (config: InternalAxiosRequestConfig) => [config.params?.timestamp, config.params?.signature];
const headers = (...names: string[]) => (config: InternalAxiosRequestConfig) =>
  names.map((name) => new AxiosHeaders(config.headers).get(name));

const VENUES: Venue[] = [
  {
    name: 'binance',
    create: () => createBinanceAPI(keys, exchange('binance')),
    write: { method: 'post', url: '/fapi/v1/order', data: { symbol: 'BTCUSDT' } },
    rejection: binanceStyle,
    ok: { orderId: 1 },
    signatureOf: binanceSignature,
  },
  {
    name: 'binancepm',
    create: () => createBinancePmAPI(keys, exchange('binancepm')),
    write: { method: 'post', url: '/papi/v1/um/order', data: { symbol: 'BTCUSDT' } },
    rejection: binanceStyle,
    ok: { orderId: 1 },
    signatureOf: binanceSignature,
  },
  {
    name: 'binancespot',
    create: () => createBinanceSpotAPI(keys, exchange('binancespot')),
    write: { method: 'post', url: '/api/v3/order', data: { symbol: 'BTCUSDT' } },
    rejection: binanceStyle,
    ok: { orderId: 1 },
    signatureOf: binanceSignature,
  },
  {
    name: 'mexc',
    create: () => createMexcAPI(keys, exchange('mexc')),
    write: { method: 'post', url: '/api/v1/private/order/submit', data: { symbol: 'BTC_USDT' } },
    rejection: { success: false, code: 602, message: 'Signature verification failed!' },
    ok: { success: true, code: 0, data: 1 },
    signatureOf: (config) => [new AxiosHeaders(config.headers).get('Request-Time'), new AxiosHeaders(config.headers).get('Signature')],
  },
  {
    name: 'bingx',
    create: () => createBingxAPI(keys, exchange('bingx')),
    write: { method: 'post', url: '/openApi/swap/v2/trade/order', data: { symbol: 'BTC-USDT' } },
    rejection: { code: 100001, msg: 'Signature verification failed' },
    ok: { code: 0, data: {} },
    signatureOf: (config) => {
      const body = typeof config.data === 'string' ? JSON.parse(config.data) : (config.data ?? config.params);
      return [body?.timestamp, body?.signature];
    },
  },
  {
    name: 'okx',
    create: () => createOkxAPI(keys, exchange('okx')),
    write: { method: 'post', url: '/api/v5/trade/order', data: { instId: 'BTC-USDT-SWAP' } },
    rejection: { code: '50113', msg: 'Invalid Sign', data: [] },
    ok: { code: '0', data: [] },
    signatureOf: headers('OK-ACCESS-TIMESTAMP', 'OK-ACCESS-SIGN'),
  },
  {
    name: 'bitget',
    create: () => createBitgetAPI(keys, exchange('bitget')),
    write: { method: 'post', url: '/api/v2/mix/order/place-order', data: { symbol: 'BTCUSDT' } },
    rejection: { code: '40009', msg: 'sign signature error' },
    ok: { code: '00000', data: {} },
    signatureOf: headers('ACCESS-TIMESTAMP', 'ACCESS-SIGN'),
  },
  {
    name: 'kucoin',
    create: () => createKucoinAPI(keys, exchange('kucoin')),
    write: { method: 'post', url: '/api/v1/orders', data: { symbol: 'XBTUSDTM' } },
    rejection: { code: '400005', msg: 'Invalid KC-API-SIGN' },
    ok: { code: '200000', data: {} },
    signatureOf: headers('KC-API-TIMESTAMP', 'KC-API-SIGN'),
  },
  {
    name: 'kucoinspot',
    create: () => createKucoinSpotAPI(keys, exchange('kucoinspot')),
    write: { method: 'post', url: '/api/v1/hf/orders', data: { symbol: 'BTC-USDT' } },
    rejection: { code: '400005', msg: 'Invalid KC-API-SIGN' },
    ok: { code: '200000', data: {} },
    signatureOf: headers('KC-API-TIMESTAMP', 'KC-API-SIGN'),
  },
  {
    name: 'bitunix',
    create: () => createBitunixAPI(keys, exchange('bitunix')),
    write: { method: 'post', url: '/api/v1/futures/trade/place_order', data: { symbol: 'BTCUSDT' } },
    rejection: { code: 10007, msg: 'Sign signature error' },
    ok: { code: 0, data: {} },
    signatureOf: headers('nonce', 'sign'),
  },
  {
    name: 'cryptocom',
    create: () => createCryptocomAPI(keys, exchange('cryptocom')),
    write: { method: 'post', url: '/private/create-order', data: { instrument_name: 'BTCUSD-PERP' } },
    rejection: { code: 40101, message: 'Not authenticated, or key/signature incorrect' },
    ok: { code: 0, result: {} },
    signatureOf: (config) => {
      const body = typeof config.data === 'string' ? JSON.parse(config.data) : config.data;
      return [body?.nonce, body?.sig];
    },
  },
  {
    name: 'krakenspot',
    create: () => createKrakenSpotAPI(keys, exchange('krakenspot')),
    write: { method: 'post', url: '/0/private/AddOrder', data: { pair: 'XBTUSD', volume: '1' } },
    rejection: { error: ['EAPI:Invalid signature'] },
    ok: { error: [], result: {} },
    signatureOf: (config) => [new AxiosHeaders(config.headers).get('API-Sign'), String(config.data)],
  },
];

type Shape = 'rejected' | 'resolved';

/** Answers with the rejection `rejections` times, in the given shape, then ok. */
const scripted = (venue: Pick<Venue, 'rejection' | 'ok'>, shape: Shape, rejections: number) => {
  const sent: InternalAxiosRequestConfig[] = [];
  const adapter = async (config: InternalAxiosRequestConfig) => {
    // axios' real http adapter refuses a body that is still an object after transformRequest —
    // and axios-retry resends with an identity transformRequest.
    if (config.data !== undefined && config.data !== null && typeof config.data !== 'string') {
      throw new TypeError('Data after transformation must be a string, an ArrayBuffer, a Buffer, or a Stream');
    }
    sent.push(config);
    if (sent.length > rejections) return { data: venue.ok, status: 200, statusText: 'OK', headers: {}, config };
    const status = shape === 'rejected' ? 400 : 200;
    const response = { data: venue.rejection, status, statusText: '', headers: {}, config };
    if (shape === 'resolved') return response;
    throw new AxiosError('Bad Request', AxiosError.ERR_BAD_REQUEST, config, null, response as any);
  };
  return { sent, adapter };
};

afterEach(() => {
  vi.restoreAllMocks();
});

describe.each(VENUES)('$name signature rejection', (venue) => {
  const advanceClock = () => {
    let now = 1_800_000_000_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockImplementation(() => (now += 1_000));
    vi.spyOn(virtualClock, 'getCurrentTime').mockImplementation(() => dayjs.utc((now += 1_000)));
  };

  it.each<Shape>(['rejected', 'resolved'])('re-signs and resends a %s rejection once', async (shape) => {
    advanceClock();
    const xhr = venue.create();
    const { sent, adapter } = scripted(venue, shape, 1);
    xhr.defaults.adapter = adapter as any;

    const response = await xhr.request(venue.write);

    expect(response.status).toBe(200);
    expect(sent).toHaveLength(2);
    expect(venue.signatureOf(sent[1])).not.toEqual(venue.signatureOf(sent[0]));
  });

  it.each<Shape>(['rejected', 'resolved'])('surfaces a second %s rejection after one resend', async (shape) => {
    advanceClock();
    const xhr = venue.create();
    const { sent, adapter } = scripted(venue, shape, Infinity);
    xhr.defaults.adapter = adapter as any;

    await xhr.request(venue.write).catch(() => undefined);

    expect(sent).toHaveLength(2);
  });
});

describe('asterdex signature rejection', () => {
  const create = () => {
    let now = 1_800_000_000_000;
    vi.spyOn(asterDEXVirtualClock, 'needsSync').mockReturnValue(false);
    vi.spyOn(asterDEXVirtualClock, 'getCurrentTimeMs').mockImplementation(() => (now += 1_000));
    const options = { tealstreetTunnel: false } as any;
    const v3SignerPrivateKey = `0x${'11'.repeat(32)}`;
    const address = ethers.computeAddress(v3SignerPrivateKey);
    return createAsterdexAPI({ ...exchange('asterdex'), options, log: () => undefined } as any, {
      apiKey: 'api-key',
      apiSecret: 'api-secret',
      v3UserAddress: address,
      v3SignerAddress: address,
      v3SignerPrivateKey,
    } as any);
  };
  const venue = { rejection: binanceStyle, ok: { orderId: 1 } };

  it.each<Shape>(['rejected', 'resolved'])('re-signs and resends a v1 %s rejection once', async (shape) => {
    const api = create();
    const { sent, adapter } = scripted(venue, shape, 1);
    api.defaults.adapter = adapter as any;

    const response = await api.post('/fapi/v1/order', { symbol: 'BTCUSDT' });

    expect(response.data).toEqual(venue.ok);
    expect(sent).toHaveLength(2);
    expect(binanceSignature(sent[1])).not.toEqual(binanceSignature(sent[0]));
  });

  it.each<Shape>(['rejected', 'resolved'])('leaves a v3 (wallet-signed) %s rejection alone', async (shape) => {
    const api = create();
    const { sent, adapter } = scripted(venue, shape, 1);
    api.defaults.adapter = adapter as any;

    await api.get('/fapi/v3/balance').catch(() => undefined);

    expect(sent).toHaveLength(1);
  });

  it.each<Shape>(['rejected', 'resolved'])('surfaces a second v1 %s rejection after one resend', async (shape) => {
    const api = create();
    const { sent, adapter } = scripted(venue, shape, Infinity);
    api.defaults.adapter = adapter as any;

    await expect(api.post('/fapi/v1/order', { symbol: 'BTCUSDT' })).rejects.toThrow();

    expect(sent).toHaveLength(2);
    expect(sent[1].params.symbol).toBe('BTCUSDT');
  });

  it('signs a replay without the previous attempt\'s signing fields when its unsigned params were never kept', async () => {
    const api = create();
    const sent: InternalAxiosRequestConfig[] = [];
    api.defaults.adapter = (async (config: InternalAxiosRequestConfig) => {
      sent.push(config);
      return { data: { orderId: 1 }, status: 200, statusText: 'OK', headers: {}, config };
    }) as any;

    await api.post('/fapi/v1/order', { symbol: 'BTCUSDT' });
    // The hosted shape: the server never records `__asterUnsignedParams`.
    await api.request({ ...sent[0], __asterUnsignedParams: undefined } as any);

    const { signature, ...signed } = sent[1].params;
    const query = Object.entries(signed)
      .map(([key, value]) => `${key}=${value}`)
      .join('&');
    expect(Object.keys(signed)).toEqual(['symbol', 'recvWindow', 'timestamp']);
    expect(signature).toBe(createHmac('sha256', 'api-secret').update(query).digest('hex'));
  });

  it('still resends a -1021 timestamp rejection after resyncing', async () => {
    const api = create();
    const sync = vi.spyOn(asterDEXVirtualClock, 'syncClientServerTime').mockResolvedValue(undefined as never);
    const timestampVenue = {
      rejection: { code: -1021, msg: 'Timestamp for this request is outside of the recvWindow.' },
      ok: { orderId: 1 },
    };
    const { sent, adapter } = scripted(timestampVenue, 'rejected', 1);
    api.defaults.adapter = adapter as any;

    await api.post('/fapi/v1/order', { symbol: 'BTCUSDT' });

    expect(sync).toHaveBeenCalled();
    expect(sent).toHaveLength(2);
  });
});

describe('binance error emission across the resend', () => {
  it('emits once when a rejected signature\'s resend comes back as a 200 error', async () => {
    const binanceExchange = { ...exchange('binance'), emitAndLogError: vi.fn() };
    const xhr = createBinanceAPI(keys, binanceExchange);
    let attempts = 0;
    xhr.defaults.adapter = (async (config: InternalAxiosRequestConfig) => {
      attempts += 1;
      if (attempts === 1) {
        const response = { data: binanceStyle, status: 400, statusText: '', headers: {}, config };
        throw new AxiosError('Bad Request', AxiosError.ERR_BAD_REQUEST, config, null, response as any);
      }
      return { data: { code: -2010, msg: 'Account has insufficient balance' }, status: 200, statusText: 'OK', headers: {}, config };
    }) as any;

    await xhr.request(VENUES[0].write).catch(() => undefined);

    expect(attempts).toBe(2);
    expect(binanceExchange.emitAndLogError).toHaveBeenCalledTimes(1);
  });

  it('emits a twice-rejected signature once, not once per pass through the chain', async () => {
    const binanceExchange = { ...exchange('binance'), emitAndLogError: vi.fn() };
    const xhr = createBinanceAPI(keys, binanceExchange);
    const { adapter } = scripted(VENUES[0], 'rejected', Infinity);
    xhr.defaults.adapter = adapter as any;

    await xhr.request(VENUES[0].write).catch(() => undefined);

    expect(binanceExchange.emitAndLogError).toHaveBeenCalledTimes(1);
  });
});

describe('cryptocom across an axios-retry resend', () => {
  it('unwraps a read that succeeds after a network error once, not twice', async () => {
    const xhr = createCryptocomAPI(keys, exchange('cryptocom'));
    let attempts = 0;
    xhr.defaults.adapter = (async (config: InternalAxiosRequestConfig) => {
      attempts += 1;
      if (attempts === 1) throw new AxiosError('socket hang up', AxiosError.ERR_NETWORK, config);
      return { data: { code: 0, result: { balance: 1 } }, status: 200, statusText: 'OK', headers: {}, config };
    }) as any;

    const response = await xhr.post('/private/user-balance', {});

    expect(attempts).toBe(2);
    expect(response.data).toEqual({ balance: 1 });
  });
});

describe('bitget error emission across an axios-retry resend', () => {
  it('emits a business error from a retried read once', async () => {
    const bitgetExchange = { ...exchange('bitget'), emitAndLogError: vi.fn() };
    const xhr = createBitgetAPI(keys, bitgetExchange);
    let attempts = 0;
    xhr.defaults.adapter = (async (config: InternalAxiosRequestConfig) => {
      attempts += 1;
      if (attempts === 1) throw new AxiosError('socket hang up', AxiosError.ERR_NETWORK, config);
      return { data: { code: '40034', msg: 'Parameter does not exist' }, status: 200, statusText: 'OK', headers: {}, config };
    }) as any;

    await xhr.get('/api/v2/mix/account/accounts');

    expect(attempts).toBe(2);
    expect(bitgetExchange.emitAndLogError).toHaveBeenCalledTimes(1);
  });
});
