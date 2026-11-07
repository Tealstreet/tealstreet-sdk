import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeAccountConfig } from '../types';

import { afterEach, expect, it, vi } from 'vitest';

import { accountToExchangeOptions } from '../createExchangeFromAccount';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { prepareRecordedHttp } from './support/managedHttp';

vi.mock('../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('./reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../exchanges/rust/betaVenues')>());
});

/**
 * The web builds a main-thread mirror with `workerMirror: true` and starts the
 * worker that owns the engine from `{ ...exchange.options, workerMirror: undefined }`.
 * The mirror's options had lost the key to signer setup, so the worker built a
 * keyless engine: public-only, no private event, no error (bear5, 2026-10-30).
 */

// A synced direct Bybit account, as saved (dummy credentials).
const BEAR5 = {
  id: 'bear5',
  name: 'bear5',
  exchange: 'bybit',
  apiKey: 'dummykey0123456789',
  apiSecret: 'dummysecret0123456789abcdefghijklmno',
  tealstreetProxy: false,
  tealstreetTunnel: false,
  proxyRegion: 'SG',
  testnet: false,
  syncId: 'sync-bear5',
} as unknown as ExchangeAccountConfig;

const tape = () => {
  const requests: Array<{ path: string; apiKey?: string }> = [];
  const sockets: string[] = [];
  const transport: Transport = {
    prepareHttp: prepareRecordedHttp,
    now: () => Date.now(),
    connect: (url, { onOpen }) => {
      sockets.push(new URL(url).pathname);
      setTimeout(onOpen, 0);
      return { send: () => {}, close: () => {} };
    },
    request: async ({ url, headers }) => {
      requests.push({ path: new URL(url).pathname, apiKey: headers['X-BAPI-API-KEY'] });
      return { status: 200, body: new TextEncoder().encode('{"retCode":0,"retMsg":"OK","result":{"list":[]}}') };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return { transport, requests, sockets };
};

const until = async (check: () => boolean) => {
  for (let i = 0; i < 200 && !check(); i++) await new Promise((resolve) => setTimeout(resolve, 10));
  expect(check()).toBe(true);
};

const exchanges: RustEngineExchange[] = [];
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
  setRustEngineTransport();
});

it("the worker started from a mirror's options gets the key and starts the private account", async () => {
  const recorded = tape();
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => recorded.transport);

  const mirror = new RustEngineExchange(
    'bybit',
    { ...accountToExchangeOptions(BEAR5, { workerMirror: true }), engine: 'rust' },
    new DefaultStore(),
  );
  exchanges.push(mirror);
  expect(mirror.isPublicDataOnly).toBe(false);

  // exchangeAccountController's worker init.
  const forwarded = structuredClone({ ...mirror.options, workerMirror: undefined });
  expect(forwarded.key).toBe('dummykey0123456789');
  const worker = new RustEngineExchange('bybit', forwarded, new DefaultStore());
  exchanges.push(worker);
  expect(worker.isPublicDataOnly).toBe(false);
  // The worker's own options keep no signing material.
  expect(worker.options.key).toBeUndefined();

  void worker.start().catch(() => undefined);
  await until(() => recorded.requests.some((request) => request.path === '/v5/account/wallet-balance'));
  const signed = recorded.requests.filter((request) => request.apiKey);
  expect(signed.map((request) => request.path)).toEqual(
    expect.arrayContaining([
      '/v5/account/info',
      '/v5/account/wallet-balance',
      '/v5/position/list',
      '/v5/order/realtime',
    ]),
  );
  expect(signed.every((request) => request.apiKey === 'dummykey0123456789')).toBe(true);
  await until(() => recorded.sockets.includes('/v5/private'));
}, 30_000);
