import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { AxiosAdapter, AxiosInstance } from 'axios';
import type { ExchangeOptions, RemoteSigner } from '../../types';

import axios from 'axios';
import { createHmac } from 'node:crypto';
import { afterEach, expect, it, vi } from 'vitest';

import vectors from '../../../../../sdk-rs/venues/bitrue/tests/fixtures/hosted-signing.json';
import { signHostedHttp } from '../../exchanges/rust/legacySigning';
import { createRemoteSigningAdapter } from '../../remoteSigningAdapter';
import { virtualClock } from '../../utils/virtualClock';
import { createAPI } from '../reference/bitrue/bitrue.api';

const stamp = 1_700_000_000_123;
const baseURL = 'https://fapi.bitrue.com';
const key = 'fixture-bitrue-public';
const secret = 'fixture-bitrue-signing-material';
type Packet = { url: string; method?: string; body: unknown; signature?: string };
const cases = vectors.flatMap((vector) => {
  const original = { ...vector, baseURL };
  if (!vector.url.startsWith('https://')) return [original];
  const url = new URL(vector.url);
  return [original, { ...vector, label: `${vector.label} relative`, baseURL: url.origin, url: url.pathname + url.search }];
});
afterEach(() => vi.restoreAllMocks());

it.each(cases)('Bitrue $label: the actual hosted adapter sends the local origin, path, body and golden MAC', async (vector) => {
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(stamp);
  const packets: Packet[] = [];
  const capture: AxiosAdapter = async (config) => {
    packets.push({ url: axios.getUri(config), method: config.method, body: config.data,
      signature: config.headers.get('X-CH-SIGN') as string | undefined });
    return { status: 200, statusText: 'OK', data: {}, headers: {}, config };
  };
  const hostedSigning = vi.fn(async (input: { kind: string; payload?: string }) => input.kind === 'credential'
    ? { credential: key } : { credential: key, signature: createHmac('sha256', secret).update(input.payload!).digest('hex') });
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '1' }) } as unknown as EngineHost;
  const remoteSigner: RemoteSigner = { sign: (request) => signHostedHttp('bitrue', {} as ExchangeOptions, true, host, request),
    isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const local = createAPI({ key, secret } as ExchangeOptions,
    { name: 'bitrue', grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: { emit() {} } } as never);
  const hosted = axios.create({ baseURL, headers: { 'Content-Type': 'application/json' },
    adapter: createRemoteSigningAdapter(remoteSigner, 'bitrue', () => capture) });
  for (const client of [local, hosted] as AxiosInstance[]) {
    await client.request({ baseURL: vector.baseURL, method: vector.method, url: vector.url,
      ...(vector.body === null ? {} : { data: vector.body }), ...(client === local ? { adapter: capture } : {}) });
  }
  expect(packets).toHaveLength(2);
  expect(packets[0].url).toBe(new URL(vector.url, vector.baseURL).href);
  expect(packets[0].signature).toBe(vector.signature);
  expect(packets[1]).toEqual(packets[0]);
  expect(hostedSigning).toHaveBeenLastCalledWith({ kind: 'http', payload: vector.payload, stampMs: stamp, revision: '1' });
});
