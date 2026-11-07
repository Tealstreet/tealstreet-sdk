import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, RemoteSignerRequest } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { signHostedHttp, signHostedWsAuth } from '../../exchanges/rust/legacySigning';

vi.mock('nanoid', () => ({ nanoid: () => 'captured-id' }));

const request = (name: ExchangeName, url = '/private', method = 'post'): RemoteSignerRequest => ({
  id: 'request-id', type: 'http', exchangeName: name,
  config: { method, baseURL: 'https://fixture.invalid', url, params: { symbol: 'BTC USDT' }, data: undefined, headers: {} },
});

function provider() {
  const hostedSigning = vi.fn(async (input: Record<string, unknown>) => {
    if (input.kind === 'credential') return { credential: 'owned-key' };
    if (input.kind === 'http') return { credential: 'owned-key', signature: 'opaque-mac' };
    return { signature: 'opaque-ws-mac' };
  });
  const hostedSigningClock = vi.fn(() => ({ stampMs: 1234, revision: '17' }));
  return { hostedSigning, hostedSigningClock, host: { hostedSigning, hostedSigningClock } as unknown as EngineHost };
}

describe('standalone Rust hosted signing facade', () => {
  it('Binance signs exact Axios query bytes with owned primary credentials, never raw options', async () => {
    const p = provider();
    const options: ExchangeOptions = { key: 'ignored-raw-key', secret: 'unused', extra: { recvWindow: 777 } };
    const input = request('binance');
    input.config!.data = { qty: '0.0100', ids: ['2', '1'], signature: 'stale' };
    const output = await signHostedHttp('binance', options, true, p.host, input);
    expect(p.hostedSigning.mock.calls).toEqual([
      [{ kind: 'credential' }],
      [{ kind: 'http', payload: 'qty=0.0100&ids=2&ids=1&timestamp=1234&recvWindow=777', stampMs: 1234, revision: '17' }],
    ]);
    expect(output).toEqual({
      id: 'request-id', config: {
        url: '/private', data: undefined,
        params: { qty: '0.0100', ids: ['2', '1'], timestamp: 1234, recvWindow: 777, signature: 'opaque-mac' },
        headers: { Accept: 'application/json, text/plain, */*', 'Content-Type': 'application/json, charset=utf-8', 'X-MBX-APIKEY': 'owned-key' },
      },
    });
    expect(input.config!.data).toEqual({ qty: '0.0100', ids: ['2', '1'], signature: 'stale' });
    expect(options.key).toBe('ignored-raw-key');
    expect(options.secret).toBe('unused');
  });

  it('Bybit signs the complete canonical preimage once and preserves serialized body and header precedence', async () => {
    const p = provider();
    const input = request('bybit');
    input.config!.data = '{"qty":"0.0100","memo":"é/+%"}';
    input.config!.headers = { 'X-Extra': 'keep', 'X-BAPI-SIGN': 'stale', 'X-BAPI-API-KEY': 'stale' };
    const output = await signHostedHttp('bybit', { key: '', secret: '', bybitSiteId: 'BRA_BTL', extra: { recvWindow: 777 } }, true, p.host, input);
    expect(p.hostedSigning).toHaveBeenLastCalledWith({
      kind: 'http', payload: '1234owned-key777{"qty":"0.0100","memo":"é/+%"}', stampMs: 1234, revision: '17',
    });
    expect(output.config).toMatchObject({
      url: '/private', data: '{"qty":"0.0100","memo":"é/+%"}',
      params: { symbol: 'BTC USDT', requestId: 'captured-id' },
      headers: { 'X-BAPI-SIGN': 'opaque-mac', 'X-BAPI-API-KEY': 'owned-key', 'X-BAPI-TIMESTAMP': '1234', 'X-BAPI-RECV-WINDOW': '777', 'X-Extra': 'keep', 'x-site-id': 'BRA_BTL' },
    });
    expect(p.hostedSigningClock).toHaveBeenCalledTimes(1);
  });

  it('a provider identity change cannot publish a signature under the earlier credential', async () => {
    const p = provider();
    p.hostedSigning.mockImplementation(async (input) => input.kind === 'credential'
      ? { credential: 'first-key' }
      : { credential: 'replacement-key', signature: 'must-not-publish' });
    await expect(signHostedHttp('bybit', { key: '', secret: '' }, true, p.host, request('bybit'))).rejects.toThrow('Signing identity changed');
    expect(p.hostedSigning).toHaveBeenCalledTimes(2);
  });

  it('public no-auth responses remain available when a complete account provider refuses', async () => {
    for (const [name, path] of [
      ['binance', '/fapi/v1/exchangeInfo'], ['binanceinverse', '/dapi/v1/exchangeInfo'],
      ['binancepm', '/fapi/v1/exchangeInfo'], ['binancespot', '/api/v3/exchangeInfo'],
      ['bybit', '/v5/market/kline'],
    ] as const) {
      const p = provider();
      p.hostedSigning.mockRejectedValue(new Error('provider-refused'));
      const output = await signHostedHttp(name, { key: '', secret: '' }, true, p.host, request(name, path, 'get'));
      expect(output.error).toBeUndefined();
      expect(output.config?.headers).toEqual(name === 'bybit' ? {} : { 'Content-Type': 'application/json, charset=utf-8' });
      expect(p.hostedSigning).not.toHaveBeenCalled();
      expect(p.hostedSigningClock).not.toHaveBeenCalled();
    }
  });

  it('unsigned listen-key requests still present the owned key but do not create a MAC', async () => {
    const p = provider();
    const output = await signHostedHttp('binanceinverse', { key: '', secret: '', testnet: true }, true, p.host,
      request('binanceinverse', '/dapi/v1/listenKey'));
    expect(p.hostedSigning.mock.calls).toEqual([[{ kind: 'credential' }]]);
    expect(p.hostedSigningClock).not.toHaveBeenCalled();
    expect(output.config?.headers?.['X-MBX-APIKEY']).toBe('owned-key');
    expect(output.config?.params).not.toHaveProperty('signature');
  });

  it('classifies Bybit public requests after Axios method defaults and case normalization', async () => {
    for (const method of ['GET', undefined]) {
      const p = provider();
      p.hostedSigning.mockRejectedValue(new Error('provider-refused'));
      const input = request('bybit', '/v5/market/kline');
      if (method === undefined) delete (input.config as Partial<NonNullable<RemoteSignerRequest['config']>>).method;
      else input.config!.method = method;
      expect((await signHostedHttp('bybit', { key: '', secret: '' }, true, p.host, input)).config?.headers).toEqual({});
      expect(p.hostedSigning).not.toHaveBeenCalled();
    }
  });

  it('incomplete public-only accounts do not invoke owned signing or restore raw options', async () => {
    const p = provider();
    const output = await signHostedHttp('bybit', { key: 'raw-key-only', secret: '' }, false, p.host, request('bybit'));
    expect(p.hostedSigning).not.toHaveBeenCalled();
    expect(p.hostedSigningClock).not.toHaveBeenCalled();
    expect(output.config?.headers).not.toHaveProperty('X-BAPI-SIGN');
    expect(output.config?.headers).not.toHaveProperty('X-BAPI-API-KEY');
  });

  it('WS uses the original preimage and format without synthesizing time or HTTP work', async () => {
    for (const outputFormat of ['hex', 'base64', 'hex-base64'] as const) {
      const p = provider();
      const output = await signHostedWsAuth(p.host, {
        id: 'ws-id', type: 'ws-auth', exchangeName: 'bybit', wsAuth: { toSign: 'GET/realtime1700000000000', outputFormat },
      });
      expect(output).toEqual({ id: 'ws-id', wsAuthSignature: 'opaque-ws-mac' });
      expect(p.hostedSigning).toHaveBeenCalledExactlyOnceWith({ kind: 'wsAuth', toSign: 'GET/realtime1700000000000', outputFormat });
      expect(p.hostedSigningClock).not.toHaveBeenCalled();
    }
  });
});
