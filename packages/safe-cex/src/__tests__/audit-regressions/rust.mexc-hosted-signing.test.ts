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
    protectedHeaders: ['ApiKey', 'Request-Time', 'Signature', 'Recv-Window', 'Content-Type', 'source'] });
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
  expect(remote.config).toEqual({ ...local.config, headers: { ...local.config!.headers, source: 'TEAL' } });
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

it.each([' \n { "vol": 1 } \t ', '\n["739113577038255616"]\r\n'])('authenticates the exact Axios adapter body for replay JSON %j', async (body) => {
  const input = request(vectors[2]); input.config!.data = body;
  const { host } = owned();
  const result = await signHostedHttp('mexc', { key: '', secret: '' }, true, host, input);
  expect(result.error).toBeUndefined();
  expect(result.config?.headers.Signature).toBe(mac(`replay-key${stamp}${result.config?.data}`));
  expect(result.config?.data).toBe(body);
});

it('retains permitted callback headers while native authentication headers take precedence', async () => {
  const { host, hostedSigning } = owned();
  hostedSigning.mockImplementation(async (input) => input.kind === 'credential' ? { credential: 'replay-key' }
    : { credential: 'replay-key', signature: mac(input.payload!), headers: [['X-Trace', 'fixture-trace']] });
  const result = await signHostedHttp('mexc', { key: '', secret: '' }, true, host, request(vectors[2]));
  expect(result.config?.headers['X-Trace']).toBe('fixture-trace');
  expect(result.config?.headers.ApiKey).toBe('replay-key');
});

it.each(['  { "note": "é/+%" } \n', '\t[ { "symbol": "BTC_USDT" } ]\r\n'])(
  'authenticates the transmitted Axios JSON bytes: %j', async (body) => {
    const { host, hostedSigning } = owned();
    const input = request(vectors[2]);
    input.config!.data = body;
    const result = await signHostedHttp('mexc', { key: '', secret: '' }, true, host, input);
    expect(result.config!.data).toBe(body);
    expect(result.config!.headers.Signature).toBe(mac(`replay-key${stamp}${result.config!.data}`));
    expect(hostedSigning).toHaveBeenLastCalledWith({ kind: 'http', payload: `replay-key${stamp}${result.config!.data}`, stampMs: stamp, revision: '1' });
  },
);

it('retains permitted signer headers while native MEXC auth and content headers take precedence', async () => {
  const { host, hostedSigning } = owned();
  hostedSigning.mockImplementation(async (input) => input.kind === 'credential' ? { credential: 'replay-key' }
    : { credential: 'replay-key', signature: mac(input.payload!), headers: [
      ['X-Trace', 'first'], ['x-trace', 'retained'], ['apikey', 'shadow'], ['signature', 'shadow'],
      ['request-time', 'shadow'], ['recv-window', 'shadow'], ['content-type', 'text/plain'], ['SoUrCe', 'shadow'],
    ] });
  const result = await signHostedHttp('mexc', { key: '', secret: '' }, true, host, request(vectors[2]));
  const headers = Object.fromEntries(Object.entries(result.config!.headers).map(([key, value]) => [key.toLowerCase(), value]));
  expect(headers['x-trace']).toBe('retained');
  expect(headers.apikey).toBe('replay-key');
  expect(headers.signature).toBe(vectors[2].signature);
  expect(headers['request-time']).toBe(String(stamp));
  expect(headers['recv-window']).toBe('20000');
  expect(headers['content-type']).toBe('application/json');
  expect(headers.source).toBe('TEAL');
});
