import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { afterEach, beforeAll, expect, it } from 'vitest';

import { deriveBridgeConstruction } from '../../exchanges/rust/construction';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const STAMP = 1_700_000_000_123;
const PRIMARY_KEY = 'hosted-primary-key';
const PRIMARY_SECRET = 'hosted-primary-secret';
const SECONDARY_SECRET = 'hosted-secondary-secret';
const ALIASES = ['binance', 'binancepm', 'binanceinverse', 'binancespot'] as const;
const live: RustEngineExchange[] = [];

beforeAll(() => setWasmEngineLoader(loadNodeEngine));
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setRustEngineTransport();
});

const mac = (payload: string) => createHmac('sha256', PRIMARY_SECRET).update(payload).digest('hex');

const fixture = (name: typeof ALIASES[number] = 'binance', overrides: Partial<ExchangeOptions> = {}) => {
  const io: string[] = [];
  const transport: Transport = {
    now: () => STAMP,
    request: async () => { io.push('http'); throw new Error('Hosted capture must not send HTTP'); },
    connect: () => { io.push('ws'); throw new Error('Hosted capture must not open sockets'); },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>),
  };
  setRustEngineTransport(() => transport);
  const input: ExchangeOptions = {
    key: PRIMARY_KEY,
    secret: PRIMARY_SECRET,
    secondary_key: 'hosted-secondary-key',
    secondary_secret: SECONDARY_SECRET,
    extra: { recvWindow: 12_345 },
    ...overrides,
  };
  const original = structuredClone(input);
  const store = new DefaultStore();
  const exchange = new RustEngineExchange(name, input, store);
  live.push(exchange);
  return { exchange, input, original, store, io };
};

const http = (
  exchangeName: typeof ALIASES[number],
  config: Partial<NonNullable<RemoteSignerRequest['config']>> = {},
): RemoteSignerRequest => ({
  id: 'http-capture',
  type: 'http',
  exchangeName,
  config: { method: 'GET', url: '/private/audit', baseURL: 'https://venue.invalid', ...config },
});

it.each(ALIASES)('actual WASM %s captures primary HTTP and exact caller WS bytes before startup with no retained secret', async (name) => {
  const { exchange, input, original, store, io } = fixture(name);
  for (const field of ['key', 'secret', 'passphrase', 'secondary_key', 'secondary_secret', 'secondary_passphrase', 'localSigner'])
    expect(exchange.options).not.toHaveProperty(field);

  const request = http(name, { params: { symbol: 'BTC USDT', side: 'BUY' } });
  const requestBefore = structuredClone(request);
  const output = await exchange.signHttpRequest(request);
  expect(output.error).toBeUndefined();
  expect(output.id).toBe(request.id);
  expect(output.config).toMatchObject({
    url: '/private/audit',
    params: {
      symbol: 'BTC USDT', side: 'BUY', timestamp: STAMP, recvWindow: 12_345,
      signature: mac(`symbol=BTC%20USDT&side=BUY&timestamp=${STAMP}&recvWindow=12345`),
    },
    headers: { 'X-MBX-APIKEY': PRIMARY_KEY },
  });
  expect(output.config?.data).toBeUndefined();
  const toSign = 'GET/realtime1700999999999'; // Caller expiry must survive; it is not the host clock.
  const bytes = createHmac('sha256', PRIMARY_SECRET).update(toSign).digest();
  for (const outputFormat of ['hex', 'base64', 'hex-base64'] as const) {
    const expected = outputFormat === 'hex' ? bytes.toString('hex')
      : outputFormat === 'base64' ? bytes.toString('base64')
        : Buffer.from(bytes.toString('hex')).toString('base64');
    await expect(exchange.signWsAuth({
      id: `ws-${outputFormat}`, type: 'ws-auth', exchangeName: name,
      wsAuth: { toSign, outputFormat },
    })).resolves.toEqual({ id: `ws-${outputFormat}`, wsAuthSignature: expected });
  }
  expect(request).toEqual(requestBefore);
  expect(input).toEqual(original);
  expect(io).toEqual([]);
  expect(store.markets).toEqual([]);
  expect(Object.values(store.loaded).some(Boolean)).toBe(false);
});

it('actual WASM capture preserves existing property positions, qs repeated arrays, JSON data precedence and retry signature placement', async () => {
  const { exchange, io } = fixture();
  const request = http('binance', {
    method: 'POST',
    params: { ignored: 'params lose to a truthy body' },
    data: JSON.stringify({
      timestamp: 1, symbol: 'A/B', signature: 'stale-signature',
      recvWindow: 2, ids: ['a b', 'c+d'], nested: { value: 'x&y' },
    }),
    headers: { 'X-Proxy-Audit': 'kept' },
  });
  const original = structuredClone(request);
  const first = await exchange.signHttpRequest(request);
  const canonical = `timestamp=${STAMP}&symbol=A%2FB&recvWindow=12345&ids=a%20b&ids=c%2Bd&nested%5Bvalue%5D=x%26y`;
  expect(first.error).toBeUndefined();
  expect(first.config?.params).toEqual({
    timestamp: STAMP, symbol: 'A/B', recvWindow: 12_345,
    ids: ['a b', 'c+d'], nested: { value: 'x&y' }, signature: mac(canonical),
  });
  expect(Object.keys(first.config!.params!)).toEqual(['timestamp', 'symbol', 'recvWindow', 'ids', 'nested', 'signature']);
  expect(first.config?.headers).toMatchObject({ 'X-MBX-APIKEY': PRIMARY_KEY, 'X-Proxy-Audit': 'kept' });
  expect(first.config?.data).toBeUndefined();
  const second = await exchange.signHttpRequest(http('binance', {
    method: 'DELETE', params: first.config!.params, headers: first.config!.headers,
  }));
  expect(second.config?.params).toEqual(first.config?.params);
  expect(Object.keys(second.config!.params!).at(-1)).toBe('signature');
  expect(request).toEqual(original);
  expect(io).toEqual([]);
});

it.each([
  ['binance', '/fapi/v1/listenKey', false, false],
  ['binancepm', '/papi/v1/listenKey', false, false],
  ['binanceinverse', '/dapi/v1/listenKey', false, false],
  ['binance', '/fapi/v1/listenKey', true, true],
  ['binancepm', '/papi/v1/listenKey', true, true],
  ['binanceinverse', '/dapi/v1/listenKey', true, false],
  ['binancespot', '/api/v3/listenKey', false, true],
] as const)('actual WASM %s %s listen-key testnet=%s signed=%s policy', async (name, url, testnet, signed) => {
  const { exchange, io } = fixture(name, { testnet });
  const output = await exchange.signHttpRequest(http(name, { method: 'PUT', url, params: { listenKey: 'existing-key' } }));
  expect(output.error).toBeUndefined();
  expect(output.config?.headers['X-MBX-APIKEY']).toBe(PRIMARY_KEY);
  expect(output.config?.params).toEqual(signed ? {
    listenKey: 'existing-key', timestamp: STAMP, recvWindow: 12_345,
    signature: mac(`listenKey=existing-key&timestamp=${STAMP}&recvWindow=12345`),
  } : { listenKey: 'existing-key' });
  expect(io).toEqual([]);
});

it('actual WASM public capture strips auth, preserves caller query and never starts account or feeds', async () => {
  const { exchange, store, io } = fixture();
  const output = await exchange.signHttpRequest(http('binance', {
    url: '/fapi/v1/exchangeInfo', params: { symbol: 'BTCUSDT' },
    headers: { 'X-MBX-APIKEY': 'stale-caller-key', Authorization: 'stale' },
  }));
  expect(output.error).toBeUndefined();
  expect(output.config?.params).toEqual({ symbol: 'BTCUSDT' });
  expect(output.config?.headers).toEqual({ 'Content-Type': 'application/json, charset=utf-8' });
  expect(io).toEqual([]);
  expect(Object.values(store.loaded).some(Boolean)).toBe(false);
});

it('actual WASM signing errors and disposal settle without exposing secrets, issuing I/O or returning a signature', async () => {
  const { exchange, io } = fixture();
  await expect(exchange.signHttpRequest({ id: 'missing', type: 'http', exchangeName: 'binance' }))
    .resolves.toEqual({ id: 'missing', error: 'Missing HTTP config for signing' });
  await expect(exchange.signWsAuth({
    id: 'format', type: 'ws-auth', exchangeName: 'binance',
    wsAuth: { toSign: 'caller-data', outputFormat: 'coinbase-jwt' },
  })).resolves.toEqual({ id: 'format', error: 'Unsupported WS auth output format' });
  // Force construction/provider use before disposal, not merely a cold disposed check.
  expect((await exchange.signHttpRequest(http('binance'))).error).toBeUndefined();
  exchange.dispose();
  const results = await Promise.all([
    exchange.signHttpRequest(http('binance')),
    exchange.signWsAuth({
      id: 'disposed-ws', type: 'ws-auth', exchangeName: 'binance',
      wsAuth: { toSign: 'caller-data', outputFormat: 'hex' },
    }),
  ]);
  expect(results).toEqual([
    { id: 'http-capture', error: 'Hosted HTTP signing failed' },
    { id: 'disposed-ws', error: 'Hosted WS auth signing failed' },
  ]);
  expect(JSON.stringify(results)).not.toContain(PRIMARY_SECRET);
  expect(JSON.stringify(results)).not.toContain(SECONDARY_SECRET);
  expect(io).toEqual([]);
});

it('actual WASM keeps Bybit provider admission closed until its separate migration lands', async () => {
  expect(deriveBridgeConstruction('bybit', { key: '', secret: '' }).dispatchSigning).toBe(false);
  const { WasmEngine } = await loadNodeEngine();
  expect(() => new WasmEngine('bybit', '{}', JSON.stringify({
    primary: { key: PRIMARY_KEY, secret: PRIMARY_SECRET },
  }))).toThrow();
});

it('actual WASM refuses WS capture without provider credentials and does not manufacture an HTTP signature', async () => {
  const { exchange, io } = fixture('binance', {
    key: '', secret: '', secondary_key: undefined, secondary_secret: undefined, publicDataOnly: true,
  });
  await expect(exchange.signWsAuth({
    id: 'no-identity', type: 'ws-auth', exchangeName: 'binance',
    wsAuth: { toSign: 'GET/realtime1700999999999', outputFormat: 'hex' },
  })).resolves.toEqual({ id: 'no-identity', error: 'Hosted WS auth signing failed' });
  const unsigned = await exchange.signHttpRequest(http('binance', { params: { symbol: 'BTCUSDT' } }));
  expect(unsigned.error).toBeUndefined();
  expect(unsigned.config?.params).toEqual({ symbol: 'BTCUSDT' });
  expect(unsigned.config?.headers).not.toHaveProperty('X-MBX-APIKEY');
  expect(io).toEqual([]);
});
