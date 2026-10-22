import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { expect, it, vi } from 'vitest';

import { signHostedHttp } from '../../exchanges/rust/legacySigning';

const aliases = ['okx', 'okxeu', 'okxdemo', 'okxeudemo'] as const;
const stamp = 1_709_209_849_123;
const timestamp = '2024-02-29T12:30:49.123Z';

it.each(aliases)('%s captures the exact OKX HTTP preimage and trusted passphrase without a TS venue', async (name) => {
  const hostedSigning = vi.fn(async (input: { kind: string }) => input.kind === 'credential'
    ? { credential: 'owned-public' }
    : { credential: 'owned-public', signature: 'owned-base64-mac', headers: [['OK-ACCESS-PASSPHRASE', 'owned-passphrase']] });
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '3' }) } as unknown as EngineHost;
  const input: RemoteSignerRequest = { id: 'capture', type: 'http', exchangeName: name,
    config: { method: 'POST', url: '/api/v5/trade/order', baseURL: 'https://fixture.invalid',
      params: { note: 'é/+%', ids: ['a b', 'c+d'] }, data: { sz: '0.0100', memo: 'é/+%' },
      headers: { 'X-Transport': 'keep', 'OK-ACCESS-KEY': 'stale', 'OK-ACCESS-PASSPHRASE': 'stale' } } };
  const before = structuredClone(input);
  const options = { key: '', secret: '', testnet: name.endsWith('demo') } as ExchangeOptions;
  const out = await signHostedHttp(name, options, true, host, input);
  expect(hostedSigning.mock.calls).toEqual([
    [{ kind: 'credential' }],
    [{ kind: 'http', payload: `${timestamp}POST/api/v5/trade/order?note=é/+%&ids[0]=a b&ids[1]=c+d{"sz":"0.0100","memo":"é/+%"}`, stampMs: stamp, revision: '3' }],
  ]);
  expect(out.config).toMatchObject({ url: '/api/v5/trade/order', data: '{"sz":"0.0100","memo":"é/+%"}',
    params: input.config!.params, headers: {
      'Content-Type': 'application/json', 'X-Transport': 'keep', 'OK-ACCESS-KEY': 'owned-public',
      'OK-ACCESS-SIGN': 'owned-base64-mac', 'OK-ACCESS-TIMESTAMP': timestamp, 'OK-ACCESS-PASSPHRASE': 'owned-passphrase',
      ...(name.endsWith('demo') ? { 'x-simulated-trading': '1' } : {}),
    } });
  expect(input).toEqual(before);
});

it.each(aliases)('%s bypasses public capture without touching provider custody', async (name) => {
  const hostedSigning = vi.fn().mockRejectedValue(new Error('must not call'));
  const host = { hostedSigning, hostedSigningClock: vi.fn() } as unknown as EngineHost;
  const out = await signHostedHttp(name, { key: '', secret: '' }, true, host, {
    id: 'public', type: 'http', exchangeName: name,
    config: { method: 'GET', url: '/api/v5/public/instruments', params: { instType: 'SWAP' }, headers: {} },
  });
  expect(out.error).toBeUndefined();
  expect(out.config?.params).toEqual({ instType: 'SWAP' });
  expect(hostedSigning).not.toHaveBeenCalled();
});
