import type { InternalAxiosRequestConfig } from 'axios';
import type { ExchangeOptions } from '../../types';

import { AxiosHeaders } from 'axios';
import { describe, expect, it, vi } from 'vitest';

import { createBinanceRequestInterceptor, finishBinanceRequest, prepareBinanceRequest, signBinanceRequest } from '../../exchanges/binance/binance.signing';
import { createAPI } from '../reference/bybit/bybit.api';
import { createBybitRequestInterceptor, finishBybitRequest, prepareBybitRequest, withBybitRequestId } from '../../exchanges/bybit/bybit.signing';
import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { virtualClock } from '../../utils/virtualClock';

vi.mock('nanoid', () => ({ nanoid: () => 'injected-request' }));

const now = 1_700_000_000_123;
const config = (value: Partial<InternalAxiosRequestConfig>): InternalAxiosRequestConfig => ({
  method: 'get',
  url: '/private',
  headers: new AxiosHeaders(),
  ...value,
});

describe('standalone request interceptors without API or venue construction', () => {
  it('preserves every Binance alias public policy and exact private callback bytes', async () => {
    for (const [name, publicPath] of [
      ['binance', '/fapi/v1/exchangeInfo'], ['binanceinverse', '/dapi/v1/exchangeInfo'],
      ['binancepm', '/fapi/v1/exchangeInfo'], ['binancespot', '/api/v3/exchangeInfo'],
    ] as const) {
      const clock = vi.fn(() => 1234);
      const payloadSigner = vi.fn(async () => ({ signature: 'opaque', headers: { 'X-Proof': name } }));
      const sign = createBinanceRequestInterceptor({ key: 'public-key', secret: '', payloadSigner }, name, clock);
      const publicConfig = await sign(config({ url: publicPath, headers: new AxiosHeaders({ Authorization: 'old' }) }));
      expect(publicConfig).toMatchObject({ safeCexSkipRemoteSigning: true, timeout: 20000 });
      expect(publicConfig.headers.toJSON()).toEqual({ 'Content-Type': 'application/json, charset=utf-8' });
      expect(clock).not.toHaveBeenCalled();
      expect(payloadSigner).not.toHaveBeenCalled();
      const privateConfig = await sign(config({ method: 'post', url: '/private', data: { ids: ['2', '1'], qty: '0.0100' } }));
      expect(payloadSigner).toHaveBeenCalledExactlyOnceWith({
        exchangeName: name, key: 'public-key', scheme: 'hmac-sha256', encoding: 'hex',
        payload: 'ids=2&ids=1&qty=0.0100&timestamp=1234&recvWindow=20000',
      });
      expect(clock).toHaveBeenCalledTimes(1);
      expect(privateConfig.params.signature).toBe('opaque');
      expect(privateConfig).not.toHaveProperty('data');
    }
  });

  it('preserves linear/PM testnet signing, inverse unsigned listen keys, and no spot exemption', async () => {
    for (const [name, path] of [
      ['binance', '/fapi/v1/listenKey'], ['binancepm', '/papi/v1/listenKey'],
      ['binanceinverse', '/dapi/v1/listenKey'], ['binancespot', '/api/v3/listenKey'],
    ] as const) {
      for (const testnet of [false, true]) {
        const clock = vi.fn(() => 1234);
        const payloadSigner = vi.fn(async () => ({ signature: 'opaque' }));
        const sign = createBinanceRequestInterceptor({ key: 'key', secret: '', payloadSigner, testnet }, name, clock);
        const original = config({ method: 'post', url: path });
        const finished = await sign(original);
        const unsigned = name === 'binanceinverse' || (name !== 'binancespot' && !testnet);
        expect(payloadSigner).toHaveBeenCalledTimes(unsigned ? 0 : 1);
        expect(clock).toHaveBeenCalledTimes(unsigned ? 0 : 1);
        if (unsigned) {
          expect(finished).toBe(original);
          expect(finished).toMatchObject({ safeCexSkipRemoteSigning: true });
        } else expect(finished.params.signature).toBe('opaque');
      }
    }
  });

  it('Bybit uses the injected clock after JSON serialization and tags public peers without changing local signing', async () => {
    const sequence: string[] = [];
    const payloadSigner = vi.fn(async () => ({ signature: 'opaque' }));
    const sign = createBybitRequestInterceptor(
      { key: 'key', secret: '', payloadSigner, extra: { recvWindow: 777 } },
      'bybit',
      () => { sequence.push('clock'); return 1234; },
    );
    const signed = await sign(config({ method: 'post', data: { toJSON() { sequence.push('json'); return { qty: '0.0100' }; } } }));
    expect(sequence).toEqual(['json', 'clock']);
    expect(payloadSigner).toHaveBeenLastCalledWith({
      exchangeName: 'bybit', key: 'key', scheme: 'hmac-sha256', encoding: 'hex', payload: '1234key777{"qty":"0.0100"}',
    });
    expect(signed.headers.get('X-BAPI-TIMESTAMP')).toBe('1234');
    const ticker = await sign(config({ url: '/v5/market/tickers', params: { symbol: 'BTCUSDT' } }));
    expect(ticker).toMatchObject({ safeCexSkipRemoteSigning: true });
    expect(ticker.params.requestId).toBe('injected-request');
    expect(payloadSigner).toHaveBeenLastCalledWith({
      exchangeName: 'bybit', key: 'key', scheme: 'hmac-sha256', encoding: 'hex',
      payload: '1234key777symbol=BTCUSDT&requestId=injected-request',
    });
  });

  it('Bybit keeps site-only KLINE and remote/keyless bypasses ahead of the injected clock', async () => {
    for (const name of ['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'] as const) {
      const clock = vi.fn(() => 1234);
      const payloadSigner = vi.fn(async () => ({ signature: 'never' }));
      const sign = createBybitRequestInterceptor({ key: 'key', secret: '', payloadSigner, bybitSiteId: 'BRA_BTL' }, name, clock);
      const publicConfig = await sign(config({ url: ENDPOINTS.KLINE }));
      expect(publicConfig.headers.toJSON()).toEqual(name === 'bybitdemo' || name === 'hyrotrader' ? {} : { 'x-site-id': 'BRA_BTL' });
      expect(clock).not.toHaveBeenCalled();
      expect(payloadSigner).not.toHaveBeenCalled();
    }
    for (const options of [{ key: '', secret: '' }, { key: 'key', secret: '' }]) {
      const clock = vi.fn(() => 1234);
      const finished = await createBybitRequestInterceptor(options, 'bybit', clock)(config({ method: 'post', data: 'original' }));
      expect(finished.params.requestId).toBe('injected-request');
      expect(finished.data).toBe('original');
      expect(clock).not.toHaveBeenCalled();
    }
  });
});

// These expected strings spell out the existing qs grammars, rather than calling
// the serializer under test to manufacture the expected signed payload.
describe('Binance reusable request shapes', () => {
  it('retains reserved property positions, repeat arrays and nested Unicode query bytes on retry', () => {
    const data = {
      recvWindow: 1,
      symbol: 'BTC USDT/雪',
      timestamp: 2,
      signature: 'old-signature',
      ids: ['9', '2'],
      filter: { z: null, a: 'x y' },
      amount: '0.0100',
    };
    const original = config({ method: 'post', params: { ignored: 'yes' }, data });
    const prepared = prepareBinanceRequest(original, () => now, 500);
    expect(prepared.payload).toBe(
      'recvWindow=500&symbol=BTC%20USDT%2F%E9%9B%AA&timestamp=1700000000123&ids=9&ids=2&filter%5Bz%5D=&filter%5Ba%5D=x%20y&amount=0.0100',
    );
    const finished = finishBinanceRequest(prepared, { signature: 'new-signature' });
    expect(Object.keys(finished.params)).toEqual(['recvWindow', 'symbol', 'timestamp', 'ids', 'filter', 'amount', 'signature']);
    expect(finished).not.toHaveProperty('data');
    expect(finished.params.signature).toBe('new-signature');
    expect(data).toMatchObject({ recvWindow: 1, timestamp: 2, signature: 'old-signature' });
    expect(original.params).toEqual({ ignored: 'yes' });
    expect(original.data).toBe(data);
  });

  it('keeps the legacy truthy-data selection and literal string object-spread semantics', () => {
    for (const data of [undefined, null, false, 0, '']) {
      expect(prepareBinanceRequest(config({ data, params: { source: 'params' } }), () => 1, 2).payload)
        .toBe('source=params&timestamp=1&recvWindow=2');
    }
    expect(prepareBinanceRequest(config({ data: [], params: { ignored: true } }), () => 1, 2).payload)
      .toBe('timestamp=1&recvWindow=2');
    expect(prepareBinanceRequest(config({ data: 'AZ', params: { ignored: true } }), () => 1, 2).payload)
      .toBe('0=A&1=Z&timestamp=1&recvWindow=2');
  });

  it('reads data before the clock and retains configured numeric spellings', () => {
    const sequence: string[] = [];
    const original = config({ data: { get symbol() { sequence.push('data'); return 'BTC'; } } });
    const prepared = prepareBinanceRequest(original, () => { sequence.push('clock'); return 10.125; }, 123.5);
    expect(sequence).toEqual(['data', 'clock']);
    expect(prepared.payload).toBe('symbol=BTC&timestamp=10.125&recvWindow=123.5');
  });

  it('preserves callback header precedence and retains the original header owner when no headers return', () => {
    const original = config({ headers: new AxiosHeaders({ 'X-Proof': 'old', 'X-MBX-APIKEY': 'old-key' }) });
    const prepared = prepareBinanceRequest(original, () => 1, 2);
    const finished = finishBinanceRequest(prepared, { signature: 'sig', headers: { 'X-Proof': 'new', 'X-MBX-APIKEY': 'callback-key' } });
    expect(finished.headers.toJSON()).toEqual({ 'X-Proof': 'new', 'X-MBX-APIKEY': 'callback-key' });
    expect(original.headers.get('X-Proof')).toBe('old');
    expect(finishBinanceRequest(prepareBinanceRequest(original, () => 1, 2), { signature: 'sig' }).headers).toBe(original.headers);
  });

  it('the existing TS signer delegates the exact payload and keeps bypasses side-effect free', async () => {
    const clock = vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);
    const payloadSigner = vi.fn(async () => ({ signature: 'provider-signature', headers: { 'X-Proof': 'provider' } }));
    const options: ExchangeOptions = { key: 'public-identifier', secret: '', payloadSigner };
    const original = config({ method: 'delete', params: { symbol: 'BTC USDT', signature: 'old' } });
    const finished = await signBinanceRequest(original, options, 'binance', 750);
    expect(payloadSigner).toHaveBeenCalledExactlyOnceWith({
      exchangeName: 'binance', key: 'public-identifier', scheme: 'hmac-sha256', encoding: 'hex',
      payload: 'symbol=BTC%20USDT&timestamp=1700000000123&recvWindow=750',
    });
    expect(finished.params.signature).toBe('provider-signature');
    expect(finished.headers.get('X-Proof')).toBe('provider');
    clock.mockClear(); payloadSigner.mockClear();
    for (const bypass of [{ ...options, key: '' }, { key: 'public-identifier', secret: '' }]) {
      expect(await signBinanceRequest(original, bypass, 'binance', 750)).toEqual(original);
    }
    expect(clock).not.toHaveBeenCalled();
    expect(payloadSigner).not.toHaveBeenCalled();
  });
});

describe('Bybit reusable request shapes', () => {
  it('preserves requestId position, indexed nested arrays and caller-owned params', () => {
    const params = { requestId: 'old', symbol: '雪 /', nested: { a: ['x y', 'z'] }, list: [3, 1], skip: undefined, empty: null };
    const original = config({ params });
    const next = withBybitRequestId(original, () => 'new-id');
    const prepared = prepareBybitRequest(next, () => now, 777);
    expect(prepared.payload).toBe(
      'requestId=new-id&symbol=%E9%9B%AA%20%2F&nested%5Ba%5D%5B0%5D=x%20y&nested%5Ba%5D%5B1%5D=z&list%5B0%5D=3&list%5B1%5D=1&empty=',
    );
    expect(params.requestId).toBe('old');
    expect(prepared.config.params).not.toBe(params);
    expect(prepared.timestamp).toBe(now);
    expect(prepared.recvWindow).toBe(777);
  });

  it('keeps JSON string bodies literal and distinguishes null from undefined', () => {
    for (const [data, expected] of [
      [' {"qty":"0.0100","text":"雪"} ', ' {"qty":"0.0100","text":"雪"} '],
      [null, 'null'],
      [undefined, undefined],
      [{ qty: '0.0100', skip: undefined, extra: [undefined, 3], symbol: '雪' }, '{"qty":"0.0100","extra":[null,3],"symbol":"雪"}'],
    ] as const) {
      const original = config({ method: 'post', data });
      const prepared = prepareBybitRequest(original, () => 10.125, 123.5);
      expect(prepared.payload).toBe(expected);
      const finished = finishBybitRequest(prepared, 'presented-key', { signature: 'sig' });
      expect(finished.data).toBe(data);
      expect([prepared.timestamp, 'presented-key', prepared.recvWindow, prepared.payload].join(''))
        .toBe(`10.125presented-key123.5${expected ?? ''}`);
    }
  });

  it('generates requestId after params getters and samples the clock after body and query serialization', () => {
    const sequence: string[] = [];
    const original = config({
      params: { get symbol() { sequence.push('params'); return 'BTC'; } },
      data: { toJSON() { sequence.push('body'); return null; } },
    });
    const next = withBybitRequestId(original, () => { sequence.push('id'); return 'new'; });
    const prepared = prepareBybitRequest(next, () => { sequence.push('clock'); return 1; }, 2);
    expect(sequence).toEqual(['params', 'id', 'body', 'clock']);
    expect(prepared.payload).toBe('symbol=BTC&requestId=new');
  });

  it('uses the original exact lowercase GET branch and native AxiosHeaders precedence', () => {
    const original = config({ method: 'GET', data: 'body', params: { query: 'unused' }, headers: new AxiosHeaders({
      'X-Proof': 'old', 'X-BAPI-SIGN': 'old', 'X-BAPI-API-KEY': 'old', 'X-BAPI-TIMESTAMP': 'old', 'X-BAPI-RECV-WINDOW': '777', 'x-site-id': 'BRA_BTL',
    }) });
    const prepared = prepareBybitRequest(original, () => now, 777);
    expect(prepared.payload).toBe('body');
    const finished = finishBybitRequest(prepared, 'presented-key', { signature: 'sig', headers: {
      'X-Proof': 'new', 'X-BAPI-SIGN': 'callback', 'X-BAPI-API-KEY': 'callback', 'X-BAPI-TIMESTAMP': 'callback',
    } });
    expect(finished.headers.toJSON()).toEqual({
      'X-Proof': 'new', 'X-BAPI-SIGN': 'sig', 'X-BAPI-API-KEY': 'presented-key', 'X-BAPI-TIMESTAMP': `${now}`, 'X-BAPI-RECV-WINDOW': '777', 'x-site-id': 'BRA_BTL',
    });
    expect(original.headers.get('X-Proof')).toBe('old');
  });

  const interceptor = (options: ExchangeOptions) => {
    const exchange = { name: 'bybit', isDemo: false, grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: { emit: vi.fn() } };
    const api = createAPI(options, exchange as never);
    const handlers = (api.interceptors.request as unknown as { handlers: { fulfilled: (config: InternalAxiosRequestConfig) => Promise<InternalAxiosRequestConfig> }[] }).handlers;
    return handlers.at(-1)!.fulfilled;
  };

  it('the existing TS interceptor delegates helper bytes to its injected signer', async () => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);
    const payloadSigner = vi.fn(async () => ({ signature: 'provider-signature', headers: { 'X-Proof': 'provider' } }));
    const sign = interceptor({ key: 'public-identifier', secret: '', payloadSigner, extra: { recvWindow: 777 } });
    const original = config({ method: 'post', url: '/v5/order/create', data: '{"qty":"0.0100"}' });
    const finished = await sign(original);
    expect(payloadSigner).toHaveBeenCalledExactlyOnceWith({
      exchangeName: 'bybit', key: 'public-identifier', scheme: 'hmac-sha256', encoding: 'hex',
      payload: '1700000000123public-identifier777{"qty":"0.0100"}',
    });
    expect(finished.params.requestId).toBe('injected-request');
    expect(finished.data).toBe(original.data);
    expect(finished.headers.get('X-BAPI-SIGN')).toBe('provider-signature');
  });

  it('retains requestId and site-only headers on public kline before clock/signing', async () => {
    const clock = vi.spyOn(virtualClock, 'getCurrentTimeMs');
    const payloadSigner = vi.fn(async () => ({ signature: 'never' }));
    const sign = interceptor({ key: 'public-identifier', secret: '', payloadSigner, bybitSiteId: 'BRA_BTL' });
    const finished = await sign(config({ method: 'get', url: ENDPOINTS.KLINE, headers: new AxiosHeaders({ 'X-BAPI-SIGN': 'old' }) }));
    expect(finished.params.requestId).toBe('injected-request');
    expect(finished.headers.toJSON()).toEqual({ 'x-site-id': 'BRA_BTL' });
    expect(clock).not.toHaveBeenCalled();
    expect(payloadSigner).not.toHaveBeenCalled();
  });
});
