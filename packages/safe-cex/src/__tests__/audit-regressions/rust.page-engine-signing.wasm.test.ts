import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { ethers } from 'ethers';
import { afterEach, beforeAll, expect, it } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

// The COSIGNED gate for page signing: the page engine the web controller builds
// (a worker mirror that turns frame-fed) answers the server's sign-reqs itself.
const STAMP = 1_700_000_000_123;
const BYBIT_KEY = 'page-primary-key';
const BYBIT_SECRET = 'page-primary-secret';
const HL_ACCOUNT = '0x1111111111111111111111111111111111111111';
const HL_AGENT_KEY = '0x0123456789012345678901234567890123456789012345678901234567890123';
const live: RustEngineExchange[] = [];

beforeAll(() => setWasmEngineLoader(loadNodeEngine));
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setRustEngineTransport();
});

/** The page engine as `createExchangeFromAccount(account, { workerMirror: true })` builds it. */
const pageEngine = async (name: ExchangeName, options: Partial<ExchangeOptions>) => {
  const sockets: string[] = [];
  const transport: Transport = {
    now: () => STAMP,
    // Frame-fed start may ask for public metadata; signing never needs it.
    request: () => new Promise(() => {}),
    connect: () => {
      sockets.push('ws');
      throw new Error('a page engine opens no venue socket');
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>),
  };
  setRustEngineTransport(() => transport);
  const exchange = new RustEngineExchange(name, { ...options, workerMirror: true } as ExchangeOptions, new DefaultStore());
  live.push(exchange);
  await expect(exchange.frameFedReady).resolves.toBe(true);
  expect(exchange.isFrameFed).toBe(true);
  return { exchange, sockets };
};

const mac = (payload: string) => createHmac('sha256', BYBIT_SECRET).update(payload).digest('hex');

it('a frame-fed Bybit page engine signs the server HTTP and ws-auth sign-reqs with its setup-held key', async () => {
  const { exchange, sockets } = await pageEngine('bybit', {
    key: BYBIT_KEY,
    secret: BYBIT_SECRET,
    extra: { recvWindow: 8_765 },
  });
  const body = '{"symbol":"BTCUSDT","qty":"0.0100"}';
  const post: RemoteSignerRequest = {
    id: 'page-http',
    type: 'http',
    exchangeName: 'bybit',
    config: { method: 'POST', baseURL: 'https://api.bybit.com', url: '/v5/order/create', data: body, headers: {} },
  };
  const signed = await exchange.signHttpRequest(post);
  expect(signed.error).toBeUndefined();
  expect(signed.config?.data).toBe(body);
  expect(signed.config?.headers).toMatchObject({
    'X-BAPI-API-KEY': BYBIT_KEY,
    'X-BAPI-TIMESTAMP': `${STAMP}`,
    'X-BAPI-SIGN': mac(`${STAMP}${BYBIT_KEY}8765${body}`),
  });

  const toSign = 'GET/realtime1700999999999';
  await expect(
    exchange.signWsAuth({ id: 'page-ws', type: 'ws-auth', exchangeName: 'bybit', wsAuth: { toSign, outputFormat: 'hex' } }),
  ).resolves.toEqual({ id: 'page-ws', wsAuthSignature: mac(toSign) });
  expect(sockets).toEqual([]);
});

it.each(['hyperliquid', 'hyperliquidspot'] as const)(
  'a frame-fed %s page engine binds its agent address for the server',
  async (name) => {
    const { exchange, sockets } = await pageEngine(name, { key: HL_ACCOUNT, secret: HL_AGENT_KEY });
    const answer = await exchange.signWsAuth({
      id: 'page-address',
      type: 'ws-auth',
      exchangeName: name,
      wsAuth: { toSign: HL_ACCOUNT, outputFormat: 'hyperliquid-address' },
    });
    expect(answer.error).toBeUndefined();
    expect(answer.wsAuthSignature?.toLowerCase()).toBe(new ethers.Wallet(HL_AGENT_KEY).address.toLowerCase());
    expect(sockets).toEqual([]);
  },
);
