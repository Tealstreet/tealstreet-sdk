vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { afterAll, afterEach, beforeAll, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { HYROTRADER_SIMULATOR_URL, HYROTRADER_SIMULATOR_WS_URL } from '../../exchanges/bybit/bybit.types';

const STAMP = 1_700_000_000_123;
const PUBLIC = 'delegated-public';
const MATERIAL = 'delegated-provider-fixture';
const aliases = ['binance', 'binancepm', 'binanceinverse', 'binancespot'] as const;
const bybit = ['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'] as const;
type Alias = typeof aliases[number] | typeof bybit[number];
const live: RustEngineExchange[] = [];
const hyroOrigins = [HYROTRADER_SIMULATOR_URL, HYROTRADER_SIMULATOR_WS_URL.public.linear, HYROTRADER_SIMULATOR_WS_URL.private];
const originals = hyroOrigins.map((value) => ({ ...value }));
beforeAll(() => hyroOrigins.forEach((value, index) => Object.assign(value, {
  livenet: ['https://hyro-fixture.invalid', 'wss://hyro-fixture.invalid/public', 'wss://hyro-fixture.invalid/private'][index],
  testnet: ['https://hyro-fixture.invalid', 'wss://hyro-fixture.invalid/public', 'wss://hyro-fixture.invalid/private'][index],
})));
afterAll(() => hyroOrigins.forEach((value, index) => Object.assign(value, originals[index])));
const mac = (text: string) => createHmac('sha256', MATERIAL).update(text).digest('hex');
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport();
});

const exchange = (name: Alias, options: Partial<ExchangeOptions>) => {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => { throw new Error('Signing-only roundtrip must not use venue transport'); });
  setRustEngineTransport(() => ({ now: () => STAMP, request: io, connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>) }));
  const result = new RustEngineExchange(name, { engine: 'rust', key: PUBLIC, secret: '', extra: { recvWindow: 8765 }, ...options }, new DefaultStore());
  live.push(result);
  return { result, io };
};
const http = (name: Alias): RemoteSignerRequest => ({ id: 'original-http', type: 'http', exchangeName: name,
  config: { method: 'GET', baseURL: 'https://fixture.invalid', url: '/private/audit', params: { symbol: 'BTC USDT', ids: ['a+b', 'x/y'] } } });

it.each(aliases)('actual WASM %s pairs keyless RemoteSigner with the browser owned provider on the unchanged wire', async (name) => {
  const browser = exchange(name, { secret: MATERIAL });
  const sign = vi.fn((work: RemoteSignerRequest) => browser.result.signWsAuth(work));
  const remoteSigner: RemoteSigner = { accountName: 'hosted-account', sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const fallback = vi.fn(() => { throw new Error('RemoteSigner must win'); });
  const server = exchange(name, { remoteSigner, payloadSigner: fallback, secret: 'unused-local-fixture' });
  const request = http(name);
  const original = structuredClone(request);
  const output = await server.result.signHttpRequest(request);
  const canonical = `symbol=BTC%20USDT&ids=a%2Bb&ids=x%2Fy&timestamp=${STAMP}&recvWindow=8765`;
  expect(output.error).toBeUndefined();
  expect(output.id).toBe('original-http');
  expect(output.config?.params).toEqual({ symbol: 'BTC USDT', ids: ['a+b', 'x/y'], timestamp: STAMP, recvWindow: 8765, signature: mac(canonical) });
  expect(output.config?.headers['X-MBX-APIKEY']).toBe(PUBLIC);
  const text = 'caller-owned-ws-preimage';
  await expect(server.result.signWsAuth({ id: 'original-ws', type: 'ws-auth', exchangeName: name, wsAuth: { toSign: text, outputFormat: 'base64' } }))
    .resolves.toEqual({ id: 'original-ws', wsAuthSignature: Buffer.from(mac(text), 'hex').toString('base64') });
  expect(sign).toHaveBeenCalledTimes(2);
  expect(sign).toHaveBeenNthCalledWith(1, expect.objectContaining({ type: 'ws-auth', exchangeName: name, accountName: 'hosted-account', wsAuth: { toSign: canonical, outputFormat: 'hex' } }));
  expect(sign).toHaveBeenNthCalledWith(2, expect.objectContaining({ type: 'ws-auth', exchangeName: name, wsAuth: { toSign: text, outputFormat: 'hex' } }));
  expect(fallback).not.toHaveBeenCalled();
  expect(request).toEqual(original);
  for (const account of [browser, server]) {
    expect(account.io).not.toHaveBeenCalled();
    expect(account.result.hasStarted).toBe(false);
    for (const field of ['key', 'secret', 'payloadSigner', 'remoteSigner']) expect(account.result.options).not.toHaveProperty(field);
  }
});

it.each(aliases)('actual WASM %s invokes PayloadSigner with owned public receiver and exact HTTP bytes', async (name) => {
  const receiverCheck = vi.fn();
  const payloadSigner = function (this: Record<string, unknown>, work: { key: string; payload: string }) {
    expect(this).toMatchObject({ key: PUBLIC, testnet: true });
    expect(this).not.toHaveProperty('secret'); expect(this).not.toHaveProperty('passphrase');
    expect(work.key).toBe(PUBLIC);
    receiverCheck(work.payload);
    return { signature: mac(work.payload), headers: { 'x-mbx-apikey': 'callback-public', 'X-Proof': 'attempt' } };
  };
  const server = exchange(name, { payloadSigner, testnet: true, secret: 'unused-local-fixture', passphrase: 'unused-passphrase-fixture' });
  const output = await server.result.signHttpRequest(http(name));
  const canonical = `symbol=BTC%20USDT&ids=a%2Bb&ids=x%2Fy&timestamp=${STAMP}&recvWindow=8765`;
  expect(output.error).toBeUndefined();
  expect(output.config?.params?.signature).toBe(mac(canonical));
  expect(receiverCheck).toHaveBeenCalledExactlyOnceWith(canonical);
  expect(output.config?.headers['X-MBX-APIKEY']).toBe('callback-public');
  expect(output.config?.headers['X-Proof']).toBe('attempt');
  expect(server.io).not.toHaveBeenCalled();
});

it.each(bybit)('actual WASM %s pairs primary keyless HTTP and WS with the unchanged browser signer', async (name) => {
  const browser = exchange(name, { secret: MATERIAL });
  const sign = vi.fn((work: RemoteSignerRequest) => browser.result.signWsAuth(work));
  const remoteSigner: RemoteSigner = { accountName: 'hosted-account', sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const server = exchange(name, { remoteSigner });
  const output = await server.result.signHttpRequest({ ...http(name), config: { ...http(name).config!, url: '/v5/order/realtime' } });
  expect(output.error).toBeUndefined();
  const id = output.config!.params!.requestId;
  const canonical = `symbol=BTC%20USDT&ids%5B0%5D=a%2Bb&ids%5B1%5D=x%2Fy&requestId=${encodeURIComponent(String(id))}`;
  expect(output.config?.headers).toMatchObject({ 'X-BAPI-API-KEY': PUBLIC, 'X-BAPI-TIMESTAMP': String(STAMP),
    'X-BAPI-SIGN': mac(`${STAMP}${PUBLIC}8765${canonical}`) });
  const expires = STAMP + 10000;
  await expect(server.result.signWsAuth({ id: 'bybit-ws', type: 'ws-auth', exchangeName: name,
    wsAuth: { toSign: `GET/realtime${expires}`, outputFormat: 'hex' } })).resolves.toEqual({ id: 'bybit-ws', wsAuthSignature: mac(`GET/realtime${expires}`) });
  expect(sign).toHaveBeenLastCalledWith(expect.objectContaining({ exchangeName: 'bybit', wsAuth: { toSign: `GET/realtime${expires}`, outputFormat: 'hex', extraData: { expires } } }));
  expect(browser.io).not.toHaveBeenCalled(); expect(server.io).not.toHaveBeenCalled();
});

it.each(['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo'] as const)('actual WASM %s keeps callback headers under declared Bybit precedence', async (name) => {
  const server = exchange(name, { payloadSigner: ({ payload }) => ({ signature: mac(payload), headers: {
    'x-bapi-api-key': 'must-not-win', 'x-bapi-sign': 'must-not-win', 'x-bapi-timestamp': '0',
    'x-bapi-recv-window': '1000', 'X-Proof': 'attempt',
  } }) });
  const output = await server.result.signHttpRequest({ ...http(name), config: { ...http(name).config!, url: '/v5/order/realtime' } });
  expect(output.error).toBeUndefined();
  expect(output.config?.headers).toMatchObject({ 'X-BAPI-API-KEY': PUBLIC, 'X-BAPI-TIMESTAMP': String(STAMP), 'X-BAPI-RECV-WINDOW': '1000', 'X-Proof': 'attempt' });
  expect(output.config?.headers['X-BAPI-SIGN']).toMatch(/^[a-f0-9]{64}$/);
  expect(server.io).not.toHaveBeenCalled();
});

it('Hyro PayloadSigner refuses typed at setup without breaking primary RemoteSigner', () => {
  let error: unknown;
  try { exchange('hyrotrader', { payloadSigner: () => ({ signature: 'ab'.repeat(32) }) }); } catch (value) { error = value; }
  expect(error).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
});
