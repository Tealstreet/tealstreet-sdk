import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';
import { endpoints, signature, STAMP } from './backpackHostedFixtures';

vi.mock('../../secondaryReadKeys', () => ({ SECONDARY_READ_KEYS_ENABLED: true }));
let stamp = STAMP;
const live: RustEngineExchange[] = [];
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport();
  stamp = STAMP;
});
function fixture(options: Partial<ExchangeOptions> = {}) {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => {
    throw new Error('No network in hosted parity fixtures');
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
    'backpack',
    { key: 'fixture-key', secret: Buffer.alloc(32, 7).toString('base64'), ...options },
    new DefaultStore(),
  );
  const logs: string[] = [];
  exchange.emitter.on('log', (message: string) => {
    logs.push(message);
  });
  live.push(exchange);
  return { exchange, io, logs };
}

it('private PayloadSigner is typed unsupported while explicit public-only callbacks confer no primary custody', async () => {
  let error: unknown;
  try { fixture({ payloadSigner: vi.fn() }); } catch (value) { error = value; }
  expect(error).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
  const sign = vi.fn();
  const remoteSigner: RemoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const publicOnly = fixture({ publicDataOnly: true, remoteSigner });
  const result = await publicOnly.exchange.signWsAuth({ id: 'public', type: 'ws-auth', exchangeName: 'backpack',
    wsAuth: { outputFormat: 'backpack-ed25519', toSign: '{"slot":"primary","payload":"fixture"}' } });
  expect(result.wsAuthSignature).toBeUndefined(); expect(result.error).toBeTruthy(); expect(sign).not.toHaveBeenCalled();
});

it('actual WASM local/device and keyless remote/origin sign every native endpoint identically', async () => {
  const browser = fixture();
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = {
    accountName: 'fixture-account',
    sign,
    isAvailable: () => true,
    onAvailabilityChange: () => () => {},
  };
  const server = fixture({ secret: '', remoteSigner });
  for (const [method, url, data, canonical] of endpoints) {
    for (const offset of [0, 1000]) {
      stamp = STAMP + offset;
      const request: RemoteSignerRequest = {
        id: 'parity',
        type: 'http',
        exchangeName: 'backpack',
        config: {
          method,
          url,
          baseURL: 'https://api.backpack.exchange',
          ...(data === undefined ? {} : { data }),
          headers: {
            'X-API-Key': 'stale',
            'x-signature': 'stale',
            'X-Timestamp': '0',
            'X-Window': '1',
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
      const payload = `${canonical}&timestamp=${stamp}&window=60000`;
      expect(remote.config?.headers).toMatchObject({
        'X-API-Key': 'fixture-key',
        'X-Signature': signature(payload),
        'X-Timestamp': String(stamp),
        'X-Window': '60000',
        'X-Fixture': 'keep',
        'X-Broker-Id': '1200',
      });
      expect(sign).toHaveBeenLastCalledWith(
        expect.objectContaining({
          wsAuth: {
            outputFormat: 'backpack-ed25519',
            toSign: JSON.stringify({ slot: 'primary', payload }),
          },
        }),
      );
      expect(request).toEqual(before);
    }
  }
  expect(browser.io).not.toHaveBeenCalled();
  expect(server.io).not.toHaveBeenCalled();
  for (const exchange of [browser.exchange, server.exchange]) {
    for (const field of ['key', 'secret', 'remoteSigner', 'payloadSigner'])
      expect(exchange.options).not.toHaveProperty(field);
  }
  expect(JSON.stringify(sign.mock.calls)).not.toContain(Buffer.alloc(32, 7).toString('base64'));
});

it('actual WASM preserves WS/secondary signatures and refuses unknown device formats', async () => {
  const browser = fixture({ secondary_key: 'read-key', secondary_secret: Buffer.alloc(32, 9).toString('base64') });
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const server = fixture({ secret: '', remoteSigner, secondary_key: 'read-key', secondary_secret: '' });
  for (const slot of ['primary', 'secondary']) {
    const payload = `instruction=subscribe&timestamp=${stamp}&window=5000`;
    const request: RemoteSignerRequest = {
      id: 'ws',
      type: 'ws-auth',
      exchangeName: 'backpack',
      wsAuth: { outputFormat: 'backpack-ed25519', toSign: JSON.stringify({ slot, payload }) },
    };
    const local = await browser.exchange.signWsAuth(request);
    const remote = await server.exchange.signWsAuth(request);
    expect(remote.error).toBeUndefined();
    expect(remote).toEqual(local);
    expect(JSON.parse(remote.wsAuthSignature!)).toEqual({
      credential: slot === 'primary' ? 'fixture-key' : 'read-key',
      signature: signature(payload, slot === 'primary' ? 7 : 9),
    });
  }
  for (const wsAuth of [
    { toSign: 'raw-seed', outputFormat: 'base64' as const },
    { toSign: '{"slot":"foreign","payload":"fixture"}', outputFormat: 'backpack-ed25519' as const },
    { toSign: '{"payload":"fixture","unknown":true}', outputFormat: 'backpack-ed25519' as const },
  ]) {
    const result = await browser.exchange.signWsAuth({
      id: 'refused',
      type: 'ws-auth',
      exchangeName: 'backpack',
      wsAuth,
    });
    expect(result.wsAuthSignature).toBeUndefined();
    expect(result.error).toBeTruthy();
    expect(result.error).not.toContain('raw-seed');
  }
  expect(browser.io).not.toHaveBeenCalled();
  expect(server.io).not.toHaveBeenCalled();
});

it('actual WASM failure/unavailability never falls back and keeps seed material out of logs/errors', async () => {
  let available = true;
  const sign = vi.fn(async () => {
    throw new Error('fixture-secret fixture-seed');
  });
  const fallback = vi.fn();
  const remoteSigner: RemoteSigner = { sign, isAvailable: () => available, onAvailabilityChange: () => () => {} };
  const server = fixture({ secret: Buffer.alloc(32, 7).toString('base64'), remoteSigner, payloadSigner: fallback });
  const request: RemoteSignerRequest = {
    id: 'failure',
    type: 'http',
    exchangeName: 'backpack',
    config: { method: 'DELETE', url: '/api/v1/order', baseURL: 'https://api.backpack.exchange', data: '{"orderId":"123"}', headers: {} },
  };
  const result = await server.exchange.signHttpRequest(request);
  expect(result.config).toBeUndefined();
  expect(result.error).toBeTruthy();
  for (const material of ['fixture-secret', 'fixture-seed', Buffer.alloc(32, 7).toString('base64')]) {
    expect(result.error).not.toContain(material);
    expect(JSON.stringify(server.logs)).not.toContain(material);
  }
  expect(sign).toHaveBeenCalledTimes(1);
  expect(fallback).not.toHaveBeenCalled();
  expect(server.io).not.toHaveBeenCalled();
  available = false;
  const unavailable = await server.exchange.signHttpRequest(request);
  expect(unavailable.config).toBeUndefined();
  expect(unavailable.error).toBeTruthy();
  expect(sign).toHaveBeenCalledTimes(1);
  expect(fallback).not.toHaveBeenCalled();
});

it('actual WASM keyless private startup sends provider-signed account reads and the exact WS subscription', async () => {
  const browser = fixture();
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const reads: Array<{ url: string; headers: Record<string, string> }> = [];
  const subscriptions: string[] = [];
  setRustEngineTransport(() => ({
    prepareHttp: prepareRecordedHttp,
    now: () => stamp,
    request: async ({ url, headers }) => {
      reads.push({ url, headers });
      const body = url.endsWith('/markets')
        ? '[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}]'
        : url.endsWith('/account')
          ? '{"leverageLimit":"10"}'
          : url.endsWith('/collateral')
            ? '{"netEquity":"1005","netEquityAvailable":"905","netEquityLocked":"100"}'
            : '[]';
      return { status: 200, body: new TextEncoder().encode(body) };
    },
    connect: (_url, handlers) => {
      queueMicrotask(() => handlers.onOpen());
      return {
        close() {},
        send(text) {
          if (text.includes('account.orderUpdate')) subscriptions.push(text);
        },
      };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  }));
  const server = new RustEngineExchange(
    'backpack',
    { key: 'fixture-key', secret: '', remoteSigner },
    new DefaultStore(),
  );
  live.push(server);
  const errors: unknown[] = [];
  void server.start().catch((error) => errors.push(error));
  await vi.waitFor(() => {
    expect(subscriptions).toHaveLength(1);
  });
  expect(errors).toEqual([]);
  expect(JSON.parse(subscriptions[0])).toEqual({
    method: 'SUBSCRIBE',
    params: ['account.orderUpdate', 'account.positionUpdate'],
    signature: [
      'fixture-key',
      signature(`instruction=subscribe&timestamp=${stamp}&window=5000`),
      String(stamp),
      '5000',
    ],
  });
  for (const [path, instruction] of [
    ['/account', 'accountQuery'],
    ['/position', 'positionQuery'],
    ['/collateral', 'collateralQuery'],
    ['/orders', 'orderQueryAll'],
  ]) {
    expect(reads.find((request) => request.url.endsWith(path))?.headers).toMatchObject({
      'X-API-Key': 'fixture-key',
      'X-Signature': signature(`instruction=${instruction}&timestamp=${stamp}&window=60000`),
    });
  }
  expect(JSON.stringify(sign.mock.calls)).not.toContain(Buffer.alloc(32, 7).toString('base64'));
});
