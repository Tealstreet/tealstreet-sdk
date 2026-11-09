import type { HostedSigningInput } from '@tealstreet/safe-cex-wasm/host';
import type { AxiosAdapter } from 'axios';
import type { RemoteSigner } from '../../types';

import axios from 'axios';
import { createHmac } from 'node:crypto';
import { afterEach, expect, it, vi } from 'vitest';

import vectors from '../../../../../sdk-rs/venues/bitrue/tests/fixtures/hosted-signing.json';
import { signHostedHttp } from '../../exchanges/rust/legacySigning';
import { BaseExchange } from '../../exchanges/base';
import { DefaultStore } from '../../store/store.base';
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
const exchanges: BaseExchange[] = [];
afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  vi.restoreAllMocks();
});

it.each(cases)('Bitrue $label: the actual hosted adapter sends the local origin, path, body and golden MAC', async (vector) => {
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(stamp);
  const packets: Packet[] = [];
  const capture: AxiosAdapter = async (config) => {
    const signature = config.headers.get('X-CH-SIGN');
    if (typeof signature !== 'string') throw new Error('Expected fixture signature header');
    packets.push({ url: axios.getUri(config), method: config.method, body: config.data,
      signature });
    return { status: 200, statusText: 'OK', data: {}, headers: {}, config };
  };
  const hostedSigning = vi.fn(async (input: HostedSigningInput) => {
    if (input.kind === 'credential') return { credential: key };
    if (input.kind !== 'http') throw new Error('Expected fixture HTTP signing request');
    return { credential: key, signature: createHmac('sha256', secret).update(input.payload).digest('hex') };
  });
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '1' }) };
  const remoteSigner: RemoteSigner = { sign: (request) => signHostedHttp('bitrue', { key: '', secret: '' }, true, host, request),
    isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const exchange = new BaseExchange('bitrue', { key, secret }, new DefaultStore());
  exchanges.push(exchange);
  const local = createAPI({ key, secret }, exchange);
  const hosted = axios.create({ baseURL, headers: { 'Content-Type': 'application/json' },
    adapter: createRemoteSigningAdapter(remoteSigner, 'bitrue', () => capture) });
  for (const client of [local, hosted]) {
    await client.request({ baseURL: vector.baseURL, method: vector.method, url: vector.url,
      ...(vector.body === null ? {} : { data: vector.body }), ...(client === local ? { adapter: capture } : {}) });
  }
  expect(packets).toHaveLength(2);
  expect(packets[0].url).toBe(new URL(vector.url, vector.baseURL).href);
  expect(packets[0].signature).toBe(vector.signature);
  expect(packets[1]).toEqual(packets[0]);
  expect(hostedSigning).toHaveBeenLastCalledWith({ kind: 'http', payload: vector.payload, stampMs: stamp, revision: '1' });
});
