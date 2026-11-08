import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { createHash, createHmac } from 'node:crypto';
import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';

vi.mock('../../secondaryReadKeys', () => ({ SECONDARY_READ_KEYS_ENABLED: true }));
const live: RustEngineExchange[] = [];
let stamp = 1_700_000_000_000;
const signature = (payload: string, material = 'secret') => createHmac('sha512', material)
  .update(createHash('sha256').update(payload).digest()).digest('base64');
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport();
});
function fixture(options: Partial<ExchangeOptions> = {}) {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => { throw new Error('No network in Kraken signing proof'); });
  setRustEngineTransport(() => ({ prepareHttp: prepareRecordedHttp, now: () => stamp, request: io, connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>) }));
  const exchange = new RustEngineExchange('kraken', { key: 'fixture-key', secret: 'c2VjcmV0',
    localSigner: { primary: { stableId: 'kraken-proof-primary' } }, ...options }, new DefaultStore());
  live.push(exchange);
  return { exchange, io };
}
const endpoints: Array<[string, string]> = [
  ['GET', '/derivatives/api/v3/accounts'], ['GET', '/derivatives/api/v3/openorders'],
  ['GET', '/derivatives/api/v3/openpositions'], ['GET', '/derivatives/api/v3/leveragepreferences'],
  ['GET', '/derivatives/api/v3/fills'], ['GET', '/api/history/v3/account-log'], ['GET', '/api/history/v3/orders'],
  ['POST', '/derivatives/api/v3/sendorder'], ['POST', '/derivatives/api/v3/cancelorder'],
  ['POST', '/derivatives/api/v3/cancelallorders'], ['POST', '/derivatives/api/v3/batchorder'],
  ['POST', '/derivatives/api/v3/editorder'], ['PUT', '/derivatives/api/v3/leveragepreferences'],
];

it('actual WASM local/device and keyless remote/origin sign every Futures REST endpoint identically', async () => {
  const browser = fixture();
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const server = fixture({ secret: '', remoteSigner });
  for (const [method, path] of endpoints) {
    for (const corrected of [false, true]) {
      stamp = corrected ? 1_700_000_001_000 : 1_700_000_000_000;
      const form = 'note=%C3%A9+%2B+%2F~&symbol=PF_XBTUSD';
      const nonce = corrected ? '18446744073709551615' : '1700000000000000';
      const request: RemoteSignerRequest = { id: 'http', type: 'http', exchangeName: 'kraken', config: {
        method, url: method === 'GET' ? `${path}?${form}` : path, baseURL: 'https://futures.kraken.com',
        ...(method === 'GET' ? {} : { data: form }), headers: { Nonce: nonce, APIKey: 'stale', Authent: 'stale' },
      } };
      const local = await browser.exchange.signHttpRequest(request);
      const remote = await server.exchange.signHttpRequest(request);
      expect(local.error, path).toBeUndefined();
      expect(remote.error, path).toBeUndefined();
      expect(remote.config, path).toEqual(local.config);
      expect(remote.config?.headers.Authent).toBe(signature(`${form}${nonce}${path.replace(/^\/derivatives/, '')}`));
    }
  }
  expect(browser.io).not.toHaveBeenCalled();
  expect(server.io).not.toHaveBeenCalled();
  expect(JSON.stringify(sign.mock.calls)).not.toMatch(/c2VjcmV0|delegated\//);
  expect(server.exchange.options).not.toHaveProperty('secret');
});

it('actual WASM WS auth routes challenge hashing through remote custody and binds secondary keys', async () => {
  const localSigner = { primary: { stableId: 'kraken-proof-primary' }, secondary: { stableId: 'kraken-proof-secondary' } };
  const browser = fixture({ secondary_key: 'fixture-read-key', secondary_secret: 'cmVhZC1zZWNyZXQ=', localSigner });
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const server = fixture({ secret: '', secondary_key: 'fixture-read-key', secondary_secret: '', remoteSigner, localSigner });
  const challenge = 'challenge-æ-123';
  const result = await server.exchange.signWsAuth({ id: 'ws', type: 'ws-auth', exchangeName: 'kraken',
    wsAuth: { outputFormat: 'kraken-challenge', toSign: challenge } });
  expect(result.error).toBeUndefined();
  expect(result.wsAuthSignature).toBe(signature(challenge));
  const read = await server.exchange.signWsAuth({ id: 'read', type: 'ws-auth', exchangeName: 'kraken',
    wsAuth: { outputFormat: 'kraken-sha512', toSign: JSON.stringify({ slot: 'secondary', payload: 'read' }) } });
  expect(read.error).toBeUndefined();
  expect(JSON.parse(read.wsAuthSignature!)).toEqual({ credential: 'fixture-read-key', signature: signature('read', 'read-secret') });
  expect(sign).toHaveBeenCalledTimes(2);
  expect(browser.io).not.toHaveBeenCalled();
  expect(server.io).not.toHaveBeenCalled();
});

it('actual WASM rejects unsupported modes and malformed or unavailable remote answers without material in errors', async () => {
  expect(() => fixture({ payloadSigner: vi.fn() })).toThrow();
  for (const answer of ['bad', 'unavailable']) {
    const sign = vi.fn(async (request: RemoteSignerRequest) => ({ id: request.id, error: 'fixture-private-material' }));
    const remoteSigner: RemoteSigner = { sign, isAvailable: () => answer !== 'unavailable', onAvailabilityChange: () => () => {} };
    const server = fixture({ secret: '', remoteSigner });
    const result = await server.exchange.signWsAuth({ id: 'failure', type: 'ws-auth', exchangeName: 'kraken',
      wsAuth: { outputFormat: 'kraken-challenge', toSign: 'challenge' } });
    expect(result.wsAuthSignature).toBeUndefined();
    expect(result.error).toBeTruthy();
    expect(result.error).not.toContain('fixture-private-material');
    expect(server.io).not.toHaveBeenCalled();
  }
});
