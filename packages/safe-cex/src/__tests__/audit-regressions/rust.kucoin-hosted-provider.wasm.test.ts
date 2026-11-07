import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';

import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';

let stamp = 1_700_000_000_123;
vi.mock('../../secondaryReadKeys', () => ({ SECONDARY_READ_KEYS_ENABLED: true }));
const live: RustEngineExchange[] = [];
const mac = (secret: string, payload: string) => createHmac('sha256', secret).update(payload).digest('base64');
const broker = { partner: 'partner', brokerName: 'broker', brokerKey: 'fixture-broker' };
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport();
  stamp = 1_700_000_000_123;
});
function fixture(options: Partial<ExchangeOptions> = {}) {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => {
    throw new Error('No network in hosted parity proof');
  });
  setRustEngineTransport(() => ({
    prepareHttp: prepareRecordedHttp,
    now: () => stamp,
    request: io,
    connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  }));
  const exchange = new RustEngineExchange(
    'kucoin',
    {
      key: 'fixture-key',
      secret: 'fixture-secret',
      passphrase: 'fixture-passphrase',
      extra: { kucoinBroker: broker },
      ...options,
    },
    new DefaultStore(),
  );
  const logs: string[] = [];
  exchange.emitter.on('log', (message) => {
    logs.push(message);
  });
  live.push(exchange);
  return { exchange, io, logs };
}

// Futures' complete signed endpoint inventory. Amend is cancel -> terminal proof
// GET -> replacement POST; WS authentication is the signed bullet-private POST,
// followed by a token-authenticated URL rather than another socket MAC.
const endpoints: Array<[string, string]> = [
  ['GET', '/api/v1/contracts/active'],
  ['GET', '/api/v1/allTickers'],
  ['POST', '/api/v1/bullet-private'],
  ['GET', '/api/v1/user-info'],
  ['GET', '/api/v1/positions'],
  ['GET', '/api/v1/account-overview?currency=USDT'],
  ['GET', '/api/v1/orders?status=active&currentPage=1&pageSize=100'],
  ['GET', '/api/v1/stopOrders?status=active&currentPage=1&pageSize=100'],
  ['GET', '/api/v2/position/getPositionMode'],
  ['GET', '/api/v2/position/getMarginMode?symbol=XBTUSDTM'],
  ['GET', '/api/v2/getCrossUserLeverage?symbol=XBTUSDTM'],
  ['GET', '/api/v1/fills?symbol=XBTUSDTM&startAt=1700000000000&endAt=1700000000123&currentPage=1&pageSize=50'],
  ['GET', '/api/v1/orders?symbol=XBTUSDTM&status=done&currentPage=1&pageSize=50'],
  ['GET', '/api/v1/transaction-history?currency=USDT&forward=false&maxCount=50'],
  ['GET', '/api/v1/funding-history?symbol=XBTUSDTM&reverse=true&forward=false&maxCount=50'],
  ['GET', '/api/v1/orders/order-1?symbol=XBTUSDTM'],
  ['POST', '/api/v1/orders'],
  ['POST', '/api/v1/st-orders'],
  ['DELETE', '/api/v1/orders/order-1?symbol=XBTUSDTM'],
  ['DELETE', '/api/v1/stopOrders/stop-1'],
  ['DELETE', '/api/v3/orders?symbol=XBTUSDTM'],
  ['DELETE', '/api/v1/stopOrders?symbol=XBTUSDTM'],
  ['POST', '/api/v2/changeCrossUserLeverage'],
  ['POST', '/api/v2/position/changeMarginMode'],
  ['POST', '/api/v2/position/switchPositionMode'],
  ['GET', '/api/v1/level2/snapshot?symbol=XBTUSDTM'],
  ['GET', '/api/v1/trade/history?symbol=XBTUSDTM'],
];

it('actual WASM KuCoin local and remote providers emit identical endpoint payloads and all MACs', async () => {
  const browser = fixture();
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = {
    accountName: 'fixture-account',
    sign,
    isAvailable: () => true,
    onAvailabilityChange: () => () => {},
  };
  const server = fixture({ secret: '', remoteSigner, extra: { kucoinBroker: { ...broker, brokerKey: '' } } });
  for (const [method, url] of endpoints) {
    for (const corrected of [false, true]) {
      stamp = corrected ? 1_700_000_001_123 : 1_700_000_000_123;
      const request: RemoteSignerRequest = {
        id: 'parity',
        type: 'http',
        exchangeName: 'kucoin',
        config: {
          method,
          url,
          baseURL: 'https://api-futures.kucoin.com',
          ...(method === 'POST' ? { data: '{"symbol":"XBTUSDTM","size":1,"memo":"é/+%"}' } : {}),
          headers: {
            'KC-API-SIGN': 'stale',
            'KC-API-PASSPHRASE': 'stale',
            'KC-API-TIMESTAMP': '0',
            'X-Fixture': 'keep',
          },
        },
      };
      const before = structuredClone(request);
      const local = await browser.exchange.signHttpRequest(request);
      const remote = await server.exchange.signHttpRequest(request);
      expect(local.error, url).toBeUndefined();
      expect(remote.error, url).toBeUndefined();
      expect(remote.config, url).toEqual(local.config);
      const preimage = `${stamp}${method}${url}${request.config!.data ?? ''}`;
      expect(remote.config?.headers['KC-API-SIGN'], url).toBe(mac('fixture-secret', preimage));
      expect(remote.config?.headers['KC-API-PASSPHRASE']).toBe(mac('fixture-secret', 'fixture-passphrase'));
      expect(remote.config?.headers['KC-API-PARTNER-SIGN']).toBe(mac('fixture-broker', `${stamp}partnerfixture-key`));
      expect(sign).toHaveBeenLastCalledWith(
        expect.objectContaining({
          wsAuth: {
            outputFormat: 'kucoin-batch',
            toSign: JSON.stringify({ slot: 'primary', payload: preimage, brokerPrefix: `${stamp}partner` }),
          },
        }),
      );
      expect(request).toEqual(before);
    }
  }
  expect(browser.io).not.toHaveBeenCalled();
  expect(server.io).not.toHaveBeenCalled();
  for (const exchange of [browser.exchange, server.exchange]) {
    for (const key of ['key', 'secret', 'passphrase']) expect(exchange.options).not.toHaveProperty(key);
    expect(JSON.stringify(exchange.options)).not.toContain('fixture-broker');
  }
  const wires = JSON.stringify(sign.mock.calls);
  for (const material of ['fixture-secret', 'fixture-passphrase', 'fixture-broker'])
    expect(wires).not.toContain(material);
});

it('actual WASM refuses raw auth formats, unknown batches and missing broker custody without publishing signatures', async () => {
  const browser = fixture({ extra: undefined });
  for (const wsAuth of [
    { toSign: 'raw-passphrase', outputFormat: 'base64' as const },
    { toSign: '{"payload":"p","unknown":true}', outputFormat: 'kucoin-batch' as const },
    { toSign: '{"payload":"p","brokerPrefix":"partner"}', outputFormat: 'kucoin-batch' as const },
  ]) {
    const result = await browser.exchange.signWsAuth({
      id: 'refused',
      type: 'ws-auth',
      exchangeName: 'kucoin',
      wsAuth,
    });
    expect(result.wsAuthSignature).toBeUndefined();
    expect(result.error).toBeTruthy();
    for (const material of ['fixture-secret', 'fixture-passphrase', 'fixture-broker', 'raw-passphrase'])
      expect(result.error).not.toContain(material);
  }
  expect(browser.io).not.toHaveBeenCalled();
  for (const material of ['fixture-secret', 'fixture-passphrase', 'fixture-broker', 'raw-passphrase'])
    expect(JSON.stringify(browser.logs)).not.toContain(material);
});

it('actual WASM secondary reads bind their own device credential and passphrase', async () => {
  const browser = fixture({
    secondary_key: 'fixture-read-key',
    secondary_secret: 'fixture-read-secret',
    secondary_passphrase: 'fixture-read-passphrase',
  });
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const server = fixture({
    secret: '',
    passphrase: undefined,
    remoteSigner,
    secondary_key: 'fixture-read-key',
    secondary_secret: '',
    secondary_passphrase: undefined,
    extra: { kucoinBroker: { ...broker, brokerKey: '' } },
  });
  const toSign = JSON.stringify({
    slot: 'secondary',
    payload: `${stamp}GET/api/v1/positions`,
    brokerPrefix: `${stamp}partner`,
  });
  const result = await server.exchange.signWsAuth({
    id: 'secondary',
    type: 'ws-auth',
    exchangeName: 'kucoin',
    wsAuth: { toSign, outputFormat: 'kucoin-batch' },
  });
  expect(result.error).toBeUndefined();
  expect(JSON.parse(result.wsAuthSignature!)).toEqual({
    credential: 'fixture-read-key',
    macs: [
      mac('fixture-read-secret', `${stamp}GET/api/v1/positions`),
      mac('fixture-read-secret', 'fixture-read-passphrase'),
      mac('fixture-broker', `${stamp}partnerfixture-read-key`),
    ],
  });
  expect(sign).toHaveBeenCalledTimes(1);
  expect(browser.io).not.toHaveBeenCalled();
  expect(server.io).not.toHaveBeenCalled();
});
