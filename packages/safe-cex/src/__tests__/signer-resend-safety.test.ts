import type { AxiosInstance, InternalAxiosRequestConfig } from 'axios';

import { AxiosHeaders } from 'axios';
import createHmac from 'create-hmac';
import qs from 'qs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createAPI as createBingxAPI } from './reference/bingx/bingx.api';
import { createAPI as createBitgetAPI } from './reference/bitget/bitget.api';
import { createAPI as createBlofinAPI } from './reference/blofin/blofin.api';
import { createAPI as createKucoinAPI } from './reference/kucoin/kucoin.api';
import { ENDPOINTS as KUCOIN } from './reference/kucoin/kucoin.types';
import { createAPI as createKrakenSpotAPI } from './reference/kraken/krakenspot.api';
import { createAPI as createWeexAPI } from './reference/weex/weex.api';
import { virtualClock } from '../utils/virtualClock';

// A retry — axios-retry, or a venue's resend on a signature rejection — replays the
// config the first attempt went out with: body already serialized, previous
// signature and timestamp still on it. The signer has to sign what the replay sends.

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

const capturing = (xhr: AxiosInstance) => {
  const sent: InternalAxiosRequestConfig[] = [];
  xhr.defaults.adapter = (async (config: InternalAxiosRequestConfig) => {
    sent.push(config);
    return { data: { code: 0, data: {} }, status: 200, statusText: 'OK', headers: {}, config };
  }) as any;
  return sent;
};

const header = (config: InternalAxiosRequestConfig, name: string) => new AxiosHeaders(config.headers).get(name);

/** First attempt, then the replay a retry makes of exactly what went out. */
const sendAndReplay = async (xhr: AxiosInstance, request: Parameters<AxiosInstance['request']>[0]) => {
  const sent = capturing(xhr);
  await xhr.request(request);
  await xhr.request(sent[0]);
  return sent;
};

const freezeClock = () => vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(1_800_000_000_000);
const advanceClock = () => {
  let now = 1_800_000_000_000;
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockImplementation(() => (now += 1_000));
};

afterEach(() => {
  vi.restoreAllMocks();
});

describe('a replayed request is signed over what it sends', () => {
  it.each([
    ['bitget', () => createBitgetAPI(keys, exchange('bitget')), 'ACCESS-SIGN', '/api/v2/mix/order/place-order'],
    ['weex', () => createWeexAPI(keys, exchange('weex')), 'ACCESS-SIGN', '/capi/v2/order/placeOrder'],
  ])('%s: both passes are signed over the body actually sent', async (_, create, signHeader, url) => {
    freezeClock();
    const sent = await sendAndReplay(create(), { method: 'post', url, data: { symbol: 'BTCUSDT', size: '1' } });

    expect(sent[1].data).toBe(sent[0].data);
    for (const config of sent) {
      const prehash = `${header(config, 'ACCESS-TIMESTAMP')}POST${url}${config.data}`;
      expect(header(config, signHeader)).toBe(createHmac('sha256', 'api-secret').update(prehash).digest('base64'));
    }
  });

  it('blofin: each attempt signs its own nonce over the body it sends', async () => {
    const sent = await sendAndReplay(createBlofinAPI(keys, exchange('blofin')), {
      method: 'post',
      url: '/api/v1/trade/order',
      data: { instId: 'BTC-USDT', size: '1' },
    });

    for (const config of sent) {
      const prehash = `/api/v1/trade/orderPOST${header(config, 'ACCESS-TIMESTAMP')}${header(config, 'ACCESS-NONCE')}${config.data}`;
      const expected = btoa(createHmac('sha256', 'api-secret').update(prehash).digest('hex'));
      expect(header(config, 'ACCESS-SIGN')).toBe(expected);
    }
  });

  it('bingx: a replay signs neither the old signature nor a re-quoted body', async () => {
    freezeClock();
    const xhr = createBingxAPI(keys, exchange('bingx'));

    const posted = await sendAndReplay(xhr, {
      method: 'post',
      url: '/openApi/swap/v2/trade/order',
      data: { symbol: 'BTC-USDT', side: 'BUY' },
    });
    expect(posted[1].data).toEqual(posted[0].data);

    const read = await sendAndReplay(xhr, { method: 'get', url: '/openApi/swap/v2/user/balance', params: { a: '1' } });
    expect(read[1].params).toEqual(read[0].params);
  });

  it('kucoin: a replay takes a fresh timestamp and signs it', async () => {
    advanceClock();
    const sent = await sendAndReplay(createKucoinAPI(keys, exchange('kucoin')), {
      method: 'post',
      url: KUCOIN.PLACE_ORDER,
      data: { symbol: 'XBTUSDTM', size: '1' },
    });

    expect(header(sent[1], 'KC-API-TIMESTAMP')).not.toBe(header(sent[0], 'KC-API-TIMESTAMP'));
    for (const config of sent) {
      const prehash = `${header(config, 'KC-API-TIMESTAMP')}POST${KUCOIN.PLACE_ORDER}${config.data}`;
      expect(header(config, 'KC-API-SIGN')).toBe(createHmac('sha256', 'api-secret').update(prehash).digest('base64'));
    }
  });

  it('kraken spot: a replay keeps its order params under a fresh nonce', async () => {
    advanceClock();
    const sent = await sendAndReplay(createKrakenSpotAPI(keys, exchange('krakenspot')), {
      method: 'post',
      url: '/0/private/AddOrder',
      data: { pair: 'XBTUSD', volume: '1' },
    });

    const [first, replay] = sent.map((config) => qs.parse(String(config.data)));
    expect(replay).toMatchObject({ pair: 'XBTUSD', volume: '1' });
    expect(replay.nonce).not.toBe(first.nonce);
  });

  it('kucoin, hosted without a broker: the server stamps a fresh timestamp over a replayed one', async () => {
    advanceClock();
    const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => undefined };
    const xhr = createKucoinAPI({ ...keys, secret: undefined, remoteSigner } as any, exchange('kucoin'));
    const sent = capturing(xhr);

    await xhr.request({ method: 'post', url: KUCOIN.PLACE_ORDER, data: { symbol: 'XBTUSDTM' } });
    await xhr.request({ ...sent[0], headers: new AxiosHeaders({ ...sent[0].headers, 'KC-API-TIMESTAMP': '1' }) });

    expect(header(sent[1], 'KC-API-TIMESTAMP')).not.toBe('1');
  });
});
