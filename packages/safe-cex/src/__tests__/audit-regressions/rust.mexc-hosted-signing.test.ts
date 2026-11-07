import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';
import { createHmac } from 'node:crypto';
import { afterEach, expect, it, vi } from 'vitest';
import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { signHostedHttp, signHostedWsAuth } from '../../exchanges/rust/legacySigning';
import { signHttpCapture } from '../../utils/signHttpCapture';
import { virtualClock } from '../../utils/virtualClock';
import { createAPI } from '../reference/mexc/mexc.api';
import { request, vectors } from '../support/mexcHostedSigning';

const stamp = 1_700_000_000_000;
const mac = (payload: string) => createHmac('sha256', 'replay-secret').update(payload).digest('hex');
const owned = () => {
  const hostedSigning = vi.fn(async (input: { kind: string; payload?: string; toSign?: string }) =>
    input.kind === 'credential' ? { credential: 'replay-key' }
      : { credential: 'replay-key', signature: mac(input.payload ?? input.toSign!) });
  return { hostedSigning, host: { hostedSigning,
    hostedSigningClock: () => ({ stampMs: stamp, revision: '1' }) } as unknown as EngineHost };
};
afterEach(() => vi.restoreAllMocks());

it('admits MEXC canonical HMAC delegates with REST-only PayloadSigner policy', () => {
  expect(deriveRemoteSigningPolicy('mexc')).toEqual({ enabled: true, payloadSupported: true, payloadRestOnly: true,
    protectedHeaders: ['ApiKey', 'Request-Time', 'Signature', 'Recv-Window', 'Content-Type'] });
  const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
  expect(() => assertBetaAccount('mexc', { key: 'replay-key', secret: '', remoteSigner })).not.toThrow();
  expect(() => assertBetaAccount('mexc', { key: 'replay-key', secret: '', payloadSigner: () => ({ signature: '' }) })).not.toThrow();
});

it.each(vectors)('MEXC $label matches local reference bytes and independent HMAC vector', async (vector) => {
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(stamp);
  const input = request(vector); const before = structuredClone(input);
  const { host, hostedSigning } = owned();
  const remote = await signHostedHttp('mexc', { key: '', secret: '' }, true, host, input);
  expect(remote.error).toBeUndefined();
  expect(hostedSigning.mock.calls).toEqual([[{ kind: 'credential' }], [{ kind: 'http', payload: vector.payload, stampMs: stamp, revision: '1' }]]);
  const local = await signHttpCapture(createAPI({ key: 'replay-key', secret: 'replay-secret' },
    { name: 'mexc', grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: { emit: () => undefined } } as never), request(vector), { preserveSerializedBody: true });
  expect(remote.config).toEqual(local.config);
  expect(remote.config?.headers.Signature).toBe(vector.signature);
  expect(input).toEqual(before);
});

it('signs WS login with only the canonical public key and milliseconds', async () => {
  const { host, hostedSigning } = owned();
  expect(await signHostedWsAuth(host, { id: 'login', type: 'ws-auth', exchangeName: 'mexc',
    wsAuth: { toSign: `replay-key${stamp}`, outputFormat: 'hex' } })).toEqual({ id: 'login', wsAuthSignature: mac(`replay-key${stamp}`) });
  expect(hostedSigning).toHaveBeenCalledExactlyOnceWith({ kind: 'wsAuth', toSign: `replay-key${stamp}`, outputFormat: 'hex' });
});

it.each([{ secondary_key: 'read-key' }, { secondary_secret: 'fixture-read-material' }, { localSigner: { secondary: {} } }])(
  'refuses unsupported secondary RemoteSigner custody before any callback: %j', (secondary) => {
    const sign = vi.fn();
    expect(() => new DelegatedSignerSetup('mexc', { key: 'replay-key', secret: '', ...secondary,
      remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} } } as ExchangeOptions))
      .toThrowError(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent', message: expect.stringContaining('MEXC secondary read keys') }));
    expect(sign).not.toHaveBeenCalled();
  });

it('keeps public endpoints unsigned and refuses a changed signing identity', async () => {
  const { host, hostedSigning } = owned();
  const input = request(vectors[0]); input.config!.url = '/api/v1/contract/detail';
  const result = await signHostedHttp('mexc', { key: '', secret: '' }, true, host, input);
  expect(result.error).toBeUndefined(); expect(hostedSigning).not.toHaveBeenCalled();
  hostedSigning.mockImplementation(async (input) => input.kind === 'credential' ? { credential: 'replay-key' }
    : { credential: 'rotated-key', signature: '00'.repeat(32) });
  await expect(signHostedHttp('mexc', { key: '', secret: '' }, true, host, request(vectors[0]))).rejects.toThrow('Signing identity changed');
});
