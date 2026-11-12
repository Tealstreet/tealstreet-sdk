import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';
import { createHmac } from 'node:crypto';
import { afterEach, expect, it, vi } from 'vitest';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';
import postVectors from '../fixtures/apexomni-post-signing.golden.json';

const now = 1_700_000_000_123;
const seed = `${'01'.repeat(64)}1b`;
const bytes = (text: string) => Array.from(new TextEncoder().encode(text));
const mac = (text: string) => createHmac('sha256', Buffer.from('fixture-material').toString('base64')).update(text).digest('base64');
const live: RustEngineExchange[] = [];
afterEach(() => { live.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine); setRustEngineTransport(); vi.restoreAllMocks(); });
function fixture(options: Partial<ExchangeOptions> = {}) {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => { throw new Error('No network in signing fixtures'); });
  setRustEngineTransport(() => ({ prepareHttp: prepareRecordedHttp, now: () => now, request: io, connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>) }));
  const exchange = new RustEngineExchange('apexomni', { key: 'fixture-public', secret: 'fixture-material',
    passphrase: 'fixture-pass', omniKeySeed: seed, ...options }, new DefaultStore());
  live.push(exchange); return { exchange, io };
}

it.each(postVectors)('actual WASM local and remote match the native POST golden: $label', async (vector) => {
  const device = fixture();
  const sign = vi.fn((input: RemoteSignerRequest) => device.exchange.signWsAuth(input));
  const origin = fixture({ secret: '', omniKeySeed: undefined,
    remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} } });
  for (const [data, contentType] of [
    [vector.body, vector.contentType],
    [new URLSearchParams([...vector.fields].sort(([a], [b]) => a < b ? 1 : -1) as [string, string][]).toString(), 'application/x-www-form-urlencoded'],
  ]) {
    const request: RemoteSignerRequest = { id: vector.label, type: 'http', exchangeName: 'apexomni', config: {
      method: 'POST', url: vector.path, baseURL: 'https://omni.apex.exchange/api', data,
      headers: { 'Content-Type': contentType },
    } };
    const local = await device.exchange.signHttpRequest(request);
    const remote = await origin.exchange.signHttpRequest(request);
    expect(local.error).toBeUndefined(); expect(remote.error).toBeUndefined();
    expect(local.config?.headers['APEX-SIGNATURE']).toBe(vector.signature);
    expect(remote.config).toEqual(local.config);
    expect(remote.config?.data).toBe(data);
  }
  expect(sign).toHaveBeenCalledTimes(2);
  expect(device.io).not.toHaveBeenCalled(); expect(origin.io).not.toHaveBeenCalled();
});

it('a shared signer preserves each native golden across a place/cancel batch', async () => {
  const device = fixture();
  const sign = vi.fn((input: RemoteSignerRequest) => device.exchange.signWsAuth(input));
  const origin = fixture({ secret: '', omniKeySeed: undefined,
    remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} } });
  for (const vector of postVectors.filter((item) => item.kind !== 'multipart')) {
    const data = new URLSearchParams([...vector.fields].sort(([a], [b]) => a < b ? 1 : -1) as [string, string][]).toString();
    const request: RemoteSignerRequest = { id: vector.label, type: 'http', exchangeName: 'apexomni', config: {
      method: 'POST', url: vector.path, baseURL: 'https://omni.apex.exchange/api', data,
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    } };
    const local = await device.exchange.signHttpRequest(request);
    const remote = await origin.exchange.signHttpRequest(request);
    expect(local.error).toBeUndefined(); expect(remote.error).toBeUndefined();
    expect(remote.config).toEqual(local.config);
    expect(remote.config?.headers['APEX-SIGNATURE']).toBe(vector.signature);
    expect(remote.config?.data).toBe(data);
  }
  expect(sign).toHaveBeenCalledTimes(4);
  expect(device.io).not.toHaveBeenCalled(); expect(origin.io).not.toHaveBeenCalled();
});

it('malformed multipart refuses before device signing or I/O', async () => {
  const sign = vi.fn();
  const origin = fixture({ secret: '', omniKeySeed: undefined,
    remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} } });
  const response = await origin.exchange.signHttpRequest({ id: 'malformed', type: 'http', exchangeName: 'apexomni', config: {
    method: 'POST', url: '/v3/set-initial-margin-rate', baseURL: 'https://omni.apex.exchange/api',
    data: postVectors[0].body, headers: { 'Content-Type': 'multipart/form-data' },
  } });
  expect(response.error).toBeTruthy(); expect(response.config).toBeUndefined();
  expect(sign).not.toHaveBeenCalled(); expect(origin.io).not.toHaveBeenCalled();
});

it.each([
  { method: 'GET', url: '/v3/account', params: { memo: 'é/+%', zero: 0 }, payload: `${now}GET/api/v3/account?memo=%C3%A9%2F%2B%25&zero=0` },
  { method: 'POST', url: '/v3/order', data: 'price=60000&signature=already-signed&size=0.001', payload: `${now}POST/api/v3/orderprice=60000&signature=already-signed&size=0.001` },
  { method: 'DELETE', url: '/v3/delete-order', params: { id: 'one' }, payload: `${now}DELETE/api/v3/delete-order?id=one` },
])('actual WASM local and keyless origin match full Apex HTTP bytes: $method $url', async (vector) => {
  const device = fixture();
  const sign = vi.fn((input: RemoteSignerRequest) => device.exchange.signWsAuth(input));
  const remoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const origin = fixture({ secret: '', omniKeySeed: undefined, remoteSigner });
  const request: RemoteSignerRequest = { id: 'http', type: 'http', exchangeName: 'apexomni', config: {
    url: vector.url, baseURL: 'https://omni.apex.exchange/api', method: vector.method, params: vector.params, data: vector.data, headers: {},
  } };
  const local = await device.exchange.signHttpRequest(request);
  const delegated = await origin.exchange.signHttpRequest(request);
  expect(local.error).toBeUndefined(); expect(delegated.error).toBeUndefined();
  expect(delegated.config).toEqual(local.config);
  expect(delegated.config?.headers['APEX-SIGNATURE']).toBe(mac(vector.payload));
  expect(sign).toHaveBeenCalledOnce();
  for (const material of ['fixture-material', 'fixture-pass', seed]) expect(JSON.stringify(sign.mock.calls)).not.toContain(material);
  expect(device.io).not.toHaveBeenCalled(); expect(origin.io).not.toHaveBeenCalled();
  expect(origin.exchange.options).not.toHaveProperty('secret');
  expect(origin.exchange.options).not.toHaveProperty('omniKeySeed');
});

it.each([0, 1, 3])('actual WASM device signs %s zkLink contracts before its WS/REST HMAC', async (count) => {
  const device = fixture();
  const sign = vi.fn((input: RemoteSignerRequest) => device.exchange.signWsAuth(input));
  const identity = { credential: 'ref:public', signing: 'ref:hmac', passphrase: 'ref:pass',
    extra: { zklinkSeed: 'ref:zk' }, publicCredential: 'fixture-public', passphraseValue: 'fixture-pass' };
  const provider = createRemoteSignerProvider({ exchangeName: 'apexomni', identities: { primary: identity },
    remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} } });
  const contracts = Array.from({ length: count }, (_, i) => [254, 0, 0, 0, 1, 0, 0, 0, i, ...Array<number>(27).fill(0)]);
  const payload: SignRequest['operations'][number] = { kind: 'sign', algorithm: 'hmacSha256', key: identity.signing,
    payload: count ? [ { kind: 'bytes', bytes: bytes(`${now}POST/api/v3/order`) },
      ...contracts.flatMap((_, index) => [{ kind: 'result' as const, index, encoding: 'hex' as const }, { kind: 'bytes' as const, bytes: bytes('&') }]) ]
      : [{ kind: 'bytes', bytes: bytes(`${now}GET/ws/accounts`) }] };
  const request: SignRequest = { receipt: { stampMs: now }, context: { venue: '', method: '', host: '', path: '' }, operations: [
    ...contracts.map((contract) => ({ kind: 'sign' as const, algorithm: 'zkLinkContract' as const, key: identity.extra.zklinkSeed,
      payload: [{ kind: 'bytes' as const, bytes: contract }] })),
    { kind: 'present', key: identity.credential }, { kind: 'present', key: identity.passphrase }, payload,
  ] };
  const result = await provider(request, { signal: new AbortController().signal });
  const signatures = result.values.slice(0, count).map((value) => Buffer.from(value).toString('hex'));
  expect(signatures.every((value) => value.length === 128)).toBe(true);
  const canonical = count ? `${now}POST/api/v3/order${signatures.map((value) => `${value}&`).join('')}` : `${now}GET/ws/accounts`;
  expect(Buffer.from(result.values[count + 2]).toString('base64')).toBe(mac(canonical));
  expect(result.values[count]).toEqual(bytes('fixture-public')); expect(result.values[count + 1]).toEqual(bytes('fixture-pass'));
  expect(device.io).not.toHaveBeenCalled();
});

it('keeps callback failures redacted and refuses non-trading zkLink payloads', async () => {
  const device = fixture();
  const result = await device.exchange.signWsAuth({ id: 'bad', type: 'ws-auth', exchangeName: 'apexomni',
    wsAuth: { outputFormat: 'apexomni-batch', toSign: JSON.stringify({ contracts: [[0]], payload: [{ contract: 0 }] }) } });
  expect(result.error).toBeTruthy(); expect(result.wsAuthSignature).toBeUndefined();
  for (const material of ['fixture-material', 'fixture-pass', seed]) expect(JSON.stringify(result)).not.toContain(material);
  expect(device.io).not.toHaveBeenCalled();
});

it('actual keyless WASM origins refuse unavailable, throwing and malformed delegates without fallback', async () => {
  for (const mode of ['unavailable', 'throw', 'malformed'] as const) {
    const sign = vi.fn(async (request: RemoteSignerRequest) => {
      if (mode === 'throw') throw new Error('fixture-material fixture-pass');
      return { id: request.id, wsAuthSignature: '{}' };
    });
    const fallback = vi.fn();
    const origin = fixture({ secret: '', omniKeySeed: undefined, payloadSigner: fallback,
      remoteSigner: { sign, isAvailable: () => mode !== 'unavailable', onAvailabilityChange: () => () => {} } });
    const output = await origin.exchange.signHttpRequest({ id: mode, type: 'http', exchangeName: 'apexomni',
      config: { url: '/v3/account', method: 'GET', baseURL: 'https://omni.apex.exchange/api', headers: {} } });
    expect(output.error).toBeTruthy(); expect(output.config).toBeUndefined();
    expect(origin.io).not.toHaveBeenCalled(); expect(fallback).not.toHaveBeenCalled();
    for (const material of ['fixture-material', 'fixture-pass', seed]) expect(JSON.stringify(output)).not.toContain(material);
  }
});

it.each(['', 'saved-public-key'])('explicitly public Apex WASM accounts ignore unused signing configuration (key=%s)', async (key) => {
  const remoteSigner = { sign: vi.fn(), isAvailable: vi.fn(), onAvailabilityChange: vi.fn() };
  const payloadSigner = vi.fn();
  for (const callbacks of [{ remoteSigner }, { payloadSigner }, { remoteSigner, payloadSigner }]) {
    const { exchange, io } = fixture({
      key, secret: '', passphrase: undefined, omniKeySeed: undefined,
      publicDataOnly: true, ...callbacks,
    });
    expect(exchange.isPublicDataOnly).toBe(true);
    const http = await exchange.signHttpRequest({ id: 'public', type: 'http', exchangeName: 'apexomni',
      config: { method: 'GET', url: '/v3/klines', baseURL: 'https://omni.apex.exchange/api', headers: {} } });
    expect(http.error).toBeUndefined();
    if (http.config === undefined) throw new Error('Expected public HTTP config');
    expect(Object.keys(http.config.headers).some((header) => /^APEX-/i.test(header))).toBe(false);
    const auth = await exchange.signWsAuth({ id: 'public-auth', type: 'ws-auth', exchangeName: 'apexomni',
      wsAuth: { outputFormat: 'apexomni-batch', toSign: JSON.stringify({ contracts: [], payload: [{ bytes: bytes(`${now}GET/ws/accounts`) }] }) } });
    expect(auth.error).toBeTruthy();
    expect(auth.wsAuthSignature).toBeUndefined();
    expect(io).not.toHaveBeenCalled();
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
  expect(remoteSigner.isAvailable).not.toHaveBeenCalled();
  expect(remoteSigner.onAvailabilityChange).not.toHaveBeenCalled();
  expect(payloadSigner).not.toHaveBeenCalled();
});
