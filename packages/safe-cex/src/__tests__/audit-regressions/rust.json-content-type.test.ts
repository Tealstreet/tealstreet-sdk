import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { expect, it, vi } from 'vitest';

import { signHostedHttp } from '../../exchanges/rust/legacySigning';

vi.mock('nanoid', () => ({ nanoid: () => 'header-control' }));

const json = 'application/json; charset=utf-8';
const options: ExchangeOptions = { key: '', secret: '', extra: { recvWindow: 777 } };
const binance = ['binance', 'binancepm', 'binanceinverse', 'binancespot', 'binanceoptions'] as const;
const bybit = ['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'] as const;
const provider = () => {
  const hostedSigning = vi.fn(async (input: { kind: string }) => input.kind === 'credential'
    ? { credential: 'owned-key' }
    : { credential: 'owned-key', signature: 'owned-mac', headers: [['APEX-PASSPHRASE', 'owned-pass']] });
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: 1234, revision: '17' }) } as unknown as EngineHost;
  return { host, hostedSigning };
};
const request = (name: RemoteSignerRequest['exchangeName'], method = 'get', url = '/private'): RemoteSignerRequest => ({
  id: 'media-type', type: 'http', exchangeName: name,
  config: { method, url, baseURL: 'https://fixture.invalid', params: { symbol: 'BTC USDT' }, headers: { 'X-Control': 'keep' } },
});

it.each(binance)('%s signed requests retain exact query signing with a valid JSON media type', async (name) => {
  for (const method of ['get', 'post', 'put', 'delete']) {
    const p = provider();
    const input = request(name, method);
    const before = structuredClone(input);
    const output = await signHostedHttp(name, options, true, p.host, input);
    expect(output.config?.headers).toMatchObject({ 'Content-Type': json, 'X-MBX-APIKEY': 'owned-key', 'X-Control': 'keep' });
    expect(p.hostedSigning).toHaveBeenLastCalledWith({ kind: 'http', stampMs: 1234, revision: '17',
      payload: 'symbol=BTC%20USDT&timestamp=1234&recvWindow=777' });
    expect(output.config?.params).toEqual({ symbol: 'BTC USDT', timestamp: 1234, recvWindow: 777, signature: 'owned-mac' });
    expect(output.config?.data).toBeUndefined();
    expect(input).toEqual(before);
  }
});

it.each([
  ['binance', '/fapi/v1/exchangeInfo'], ['binancepm', '/fapi/v1/exchangeInfo'],
  ['binanceinverse', '/dapi/v1/exchangeInfo'], ['binancespot', '/api/v3/exchangeInfo'],
  ['binanceoptions', '/eapi/v1/exchangeInfo'],
] as const)('%s public interceptor emits valid JSON without signing', async (name, path) => {
  const p = provider();
  const output = await signHostedHttp(name, options, true, p.host, request(name, 'get', path));
  expect(output.config?.headers).toEqual({ 'Content-Type': json });
  expect(p.hostedSigning).not.toHaveBeenCalled();
});

it.each(bybit)('%s preserves signed body/query and kline header policy', async (name) => {
  for (const method of ['get', 'post', 'put', 'delete']) {
    const p = provider();
    const input = request(name, method);
    if (method !== 'get') input.config!.data = '{"qty":"0.0100","memo":"é/+%"}';
    const before = structuredClone(input);
    const output = await signHostedHttp(name, options, true, p.host, input);
    expect(output.config?.headers).toMatchObject({ 'Content-Type': json, 'X-Control': 'keep',
      'X-BAPI-API-KEY': 'owned-key', 'X-BAPI-SIGN': 'owned-mac', 'X-BAPI-TIMESTAMP': '1234', 'X-BAPI-RECV-WINDOW': '777' });
    expect(p.hostedSigning).toHaveBeenLastCalledWith({ kind: 'http', stampMs: 1234, revision: '17',
      payload: `1234owned-key777${method === 'get' ? 'symbol=BTC%20USDT&requestId=header-control' : input.config!.data}` });
    expect(output.config?.data).toBe(input.config!.data);
    expect(output.config?.params).toEqual({ symbol: 'BTC USDT', requestId: 'header-control' });
    expect(input).toEqual(before);
  }
  const p = provider();
  const output = await signHostedHttp(name, options, true, p.host, request(name, 'get', '/v5/market/kline'));
  expect(output.config?.headers).toEqual({});
  expect(p.hostedSigning).not.toHaveBeenCalled();
});

it.each(['get', 'delete'])('ApeX Omni %s retains its exact canonical preimage with valid JSON', async (method) => {
  const p = provider();
  const input = request('apexomni', method, '/v3/account');
  const before = structuredClone(input);
  const output = await signHostedHttp('apexomni', options, true, p.host, input);
  expect(output.config?.headers).toMatchObject({ 'Content-Type': json, 'X-Control': 'keep',
    'APEX-API-KEY': 'owned-key', 'APEX-SIGNATURE': 'owned-mac', 'APEX-PASSPHRASE': 'owned-pass', 'APEX-TIMESTAMP': '1234' });
  expect(p.hostedSigning).toHaveBeenCalledExactlyOnceWith({ kind: 'http', stampMs: 1234, revision: '17',
    payload: JSON.stringify({ contracts: [], payload: [{ bytes: Array.from(new TextEncoder()
      .encode(`1234${method.toUpperCase()}/api/v3/account?symbol=BTC%20USDT`)) }] }) });
  expect(output.config?.params).toEqual(input.config!.params);
  expect(output.config?.data).toBeUndefined();
  expect(input).toEqual(before);
});

it('ApeX Omni preserves form bytes and header-free unsigned klines', async () => {
  const p = provider();
  const input = request('apexomni', 'post', '/v3/delete-order');
  input.config!.data = 'id=42';
  input.config!.headers!['Content-Type'] = 'application/x-www-form-urlencoded; charset=UTF-8';
  const output = await signHostedHttp('apexomni', options, true, p.host, input);
  expect(output.config?.headers['Content-Type']).toBe(input.config!.headers!['Content-Type']);
  expect(output.config?.data).toBe('id=42');
  p.hostedSigning.mockClear();
  const unsigned = await signHostedHttp('apexomni', options, true, p.host, request('apexomni', 'get', '/v3/klines'));
  expect(unsigned.config?.headers).toEqual({});
  expect(p.hostedSigning).not.toHaveBeenCalled();
});
