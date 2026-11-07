vi.mock('../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('./reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../exchanges/rust/betaVenues')>());
});

import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { afterEach, expect, it, vi } from 'vitest';

import { setProxyToken, setTokenRefreshFunction } from '../accessToken';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { tealstreetTransport } from '../transport/tealstreet/tealstreetTransport';
import { PRIVATE_PROXY_JP } from '../transport/tealstreet/transportResolution';
import { prepareRecordedHttp } from './support/managedHttp';

/**
 * A proxied Bybit account whose proxy token is still warming (the browser
 * SharedWorker answers '' until /api/proxykey returns) must not send its
 * startup reads without proxy authorization. The edge answers those with a
 * plain-text 401, which the engine reads as Bybit refusing the key: the
 * silent zeros of 2026-10-30. Headless: a real WASM engine, a fake edge.
 */

const jwt = (subject: string) =>
  ['{"alg":"none"}', JSON.stringify({ sub: subject, exp: 4_102_444_800 })]
    .map((part) => Buffer.from(part).toString('base64url'))
    .concat('sig')
    .join('.');
const TOKEN = jwt('user');
const EDGE_REFUSAL = 'Authorization header is missing';

const list = (rows: unknown[]) => ({ list: rows, nextPageCursor: '' });
const BYBIT: Record<string, unknown> = {
  '/v5/market/instruments-info': list([
    {
      symbol: 'BTCUSDT',
      status: 'Trading',
      baseCoin: 'BTC',
      quoteCoin: 'USDT',
      priceFilter: { tickSize: '0.10' },
      lotSizeFilter: { qtyStep: '0.001', minOrderQty: '0.001', maxOrderQty: '120' },
    },
  ]),
  '/v5/market/tickers': list([{ symbol: 'BTCUSDT', lastPrice: '90000', bid1Price: '89999.9', ask1Price: '90000.1' }]),
  '/v5/account/info': { marginMode: 'REGULAR_MARGIN', unifiedMarginStatus: 6 },
  '/v5/account/wallet-balance': list([
    {
      accountType: 'UNIFIED',
      totalWalletBalance: '1000',
      totalAvailableBalance: '1000',
      totalInitialMargin: '0',
      totalPerpUPL: '0',
    },
  ]),
  '/v5/account/fee-rate': list([]),
  '/v5/order/realtime': list([]),
  '/v5/position/list': list([]),
};

/** The Tealstreet edge in front of Bybit: no `Bearer` header is a plain-text 401. */
const edge = (refuseOnce?: string) => {
  const sent: Array<{ path: string; authorization?: string; status: number }> = [];
  let refused = false;
  const transport: Transport = {
    prepareHttp: prepareRecordedHttp,
    now: () => Date.now(),
    connect: (_url, { onOpen }) => {
      setTimeout(onOpen, 0);
      return { send: () => {}, close: () => {} };
    },
    request: async ({ url, headers }) => {
      expect(url.startsWith(`${PRIVATE_PROXY_JP}/https://api.bybit.com/`)).toBe(true);
      const path = new URL(url.slice(PRIVATE_PROXY_JP.length + 1)).pathname;
      const authorization = headers.Authorization;
      const refuse = !authorization?.startsWith('Bearer ') || (path === refuseOnce && !refused);
      if (path === refuseOnce && authorization) refused = true;
      const status = refuse ? 401 : 200;
      sent.push({ path, authorization, status });
      if (refuse) return { status, headers: '', body: new TextEncoder().encode(EDGE_REFUSAL) };
      if (!(path in BYBIT)) throw new Error(`Unexpected Bybit request: ${path}`);
      const result = path === '/v5/market/instruments-info' && url.includes('status=') ? list([]) : BYBIT[path];
      return {
        status,
        body: new TextEncoder().encode(JSON.stringify({ retCode: 0, retMsg: 'OK', result, retExtInfo: {} })),
      };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return { transport, sent };
};

const account = () => {
  const store = new DefaultStore();
  const exchange = new RustEngineExchange(
    'bybit',
    { key: 'k', secret: 's', tealstreetProxy: true, tealstreetTunnel: false, transport: tealstreetTransport },
    store,
  );
  const events: Array<{ code: string; message: string }> = [];
  const errors: string[] = [];
  exchange.emitter.on('engine-event', (event: { code: string; message: string }) => events.push(event));
  exchange.emitter.on('error', (error: unknown) => errors.push(String(error)));
  return { exchange, store, events, errors };
};

/** The SharedWorker while warming: '' until `ready` lookups have passed. */
const warmingToken = (ready: number) => {
  let lookups = 0;
  const refresh = vi.fn(async () => (++lookups > ready ? TOKEN : ''));
  setTokenRefreshFunction(refresh);
  return refresh;
};

let exchange: RustEngineExchange | undefined;
afterEach(() => {
  exchange?.dispose();
  exchange = undefined;
  setRustEngineTransport();
  setTokenRefreshFunction(null);
  setProxyToken('');
});

it('waits for a warming proxy token, so no startup read leaves without proxy authorization', async () => {
  warmingToken(2);
  const tape = edge();
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => tape.transport);
  const subject = account();
  exchange = subject.exchange;

  await exchange.start();

  const paths = tape.sent.map((request) => request.path);
  for (const read of ['/v5/account/info', '/v5/account/wallet-balance', '/v5/position/list', '/v5/order/realtime'])
    expect(paths).toContain(read);
  expect(tape.sent.every((request) => request.authorization === `Bearer ${TOKEN}` && request.status === 200)).toBe(
    true,
  );
  expect(subject.store.balance.total).toBe(1000);
  expect(subject.events.filter((event) => event.code === 'auth.rejected')).toEqual([]);
}, 30_000);

it('treats the edge refusing our token as Tealstreet authorization, refreshing it, not as a rejected key', async () => {
  const refresh = warmingToken(0);
  const tape = edge('/v5/account/wallet-balance');
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => tape.transport);
  const subject = account();
  exchange = subject.exchange;

  await exchange.start();

  const balances = tape.sent.filter((request) => request.path === '/v5/account/wallet-balance');
  expect(balances.map((request) => request.status)).toEqual([401, 200]);
  expect(subject.store.balance.total).toBe(1000);
  expect(subject.events.filter((event) => event.code === 'auth.rejected')).toEqual([]);
  expect(subject.errors).toContainEqual(
    expect.stringContaining(
      '[bybit] Tealstreet proxy refused our proxy authorization (HTTP 401: Authorization header is missing)',
    ),
  );
  expect(refresh.mock.calls.length).toBeGreaterThan(1);
}, 30_000);
