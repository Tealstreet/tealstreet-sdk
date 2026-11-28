import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { expect, it, vi } from 'vitest';

import { signHostedHttp } from '../../exchanges/rust/legacySigning';

const stamp = 1_709_209_849_123;
const owned = (input: { kind: string }) =>
  input.kind === 'credential' ? { credential: 'owned-public' } : { credential: 'owned-public', signature: 'owned-mac' };

const sign = (name: 'bybit' | 'hyrotrader') => {
  const hostedSigning = vi.fn(async (input: { kind: string }) => owned(input));
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }) } as unknown as EngineHost;
  const request: RemoteSignerRequest = {
    id: 'capture',
    type: 'http',
    exchangeName: name,
    config: { method: 'GET', baseURL: 'https://fixture.invalid', url: '/v5/order/realtime', params: { category: 'linear' } },
  };
  return signHostedHttp(name, { key: '', secret: '' } as ExchangeOptions, true, host, request);
};

// Hyro's CORS allowlist (Content-Type plus the four X-BAPI headers, checked
// 2026-11-28) refuses Bybit's broker X-Referer, so hosted signing must not add
// it for hyrotrader (TEAL-22); the Rust signer omits it the same way.
it('hyrotrader hosted HTTP signing carries no Bybit broker X-Referer', async () => {
  const out = await sign('hyrotrader');
  expect(out.error).toBeUndefined();
  expect(out.config?.headers).toMatchObject({ 'X-BAPI-API-KEY': 'owned-public', 'X-BAPI-SIGN': 'owned-mac' });
  expect(out.config?.headers).not.toHaveProperty('X-Referer');
});

it('bybit hosted HTTP signing keeps the broker X-Referer', async () => {
  const out = await sign('bybit');
  expect(out.error).toBeUndefined();
  expect(out.config?.headers).toMatchObject({ 'X-Referer': 'Tealstreet', 'X-BAPI-SIGN': 'owned-mac' });
});
