import type { HostedSigningInput, SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { InternalAxiosRequestConfig } from 'axios';
import type { RemoteSigner, RemoteSignerRequest } from '../../types';

import { AxiosHeaders } from 'axios';
import { createHmac } from 'node:crypto';
import { expect, it, vi } from 'vitest';

import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { deriveBridgeConstruction, deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { prepareBitrueRequest, signHostedHttp } from '../../exchanges/rust/legacySigning';
import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import vectors from '../../../../../sdk-rs/venues/bitrue/tests/fixtures/hosted-signing.json';

// Preserve configured read keys at the constructor boundary; the global bypass
// would otherwise discard them before delegated custody validation.
vi.mock('../../secondaryReadKeys', () => ({ SECONDARY_READ_KEYS_ENABLED: true }));

const stamp = 1_700_000_000_123;
const key = 'fixture-bitrue-public';
const secret = 'fixture-bitrue-signing-material';
const mac = (payload: string) => createHmac('sha256', secret).update(payload).digest('hex');
const identity = { credential: 'owned/public', signing: 'owned/hmac', publicCredential: key };
const remote = (sign: RemoteSigner['sign'] = async ({ id, wsAuth }) => {
  if (!wsAuth) throw new Error('Expected fixture WS-auth request');
  return { id, wsAuthSignature: mac(wsAuth.toSign) };
}): RemoteSigner => ({
  sign: vi.fn(sign), isAvailable: () => true, onAvailabilityChange: () => () => {},
});
const request = (payload: string): SignRequest => ({
  receipt: { stampMs: stamp }, context: { venue: 'bitrue', method: 'POST', host: 'fapi.bitrue.com', path: '/fixture' },
  operations: [{ kind: 'present', key: identity.credential }, { kind: 'sign', key: identity.signing,
    algorithm: 'hmacSha256', payload: [{ kind: 'bytes', bytes: Array.from(new TextEncoder().encode(payload)) }] }],
});

it('admits Bitrue and Crypto.com through the shared declaration', () => {
  const remoteSigner = remote();
  expect(deriveRemoteSigningPolicy('bitrue')).toMatchObject({ enabled: true, payloadSupported: true, payloadRestOnly: true,
    protectedHeaders: ['X-CH-APIKEY', 'X-CH-SIGN', 'X-CH-TS', 'recvWindow', 'Content-Type', 'X-Referer'] });
  expect(deriveBridgeConstruction('bitrue', { key: '', secret: '' }).dispatchSigning).toBe(true);
  // restored by #3502; #3317 dropped a live venue without a product decision.
  expect(() => assertBetaAccount('bitrue', { key, secret: '', remoteSigner })).not.toThrow();
  // restored by #3502; #3317 dropped a live venue without a product decision.
  expect(() => assertBetaAccount('bitrue', { key, payloadSigner: async () => ({ signature: '' }) })).not.toThrow();
  expect(() => assertBetaAccount('cryptocom', { key, remoteSigner })).not.toThrow();
  expect(() => assertBetaAccount('polymarket', { key, remoteSigner })).toThrow();
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

it.each(vectors)('Bitrue $label: remote and payload provider MACs match the independent golden vector', async (vector) => {
  expect(mac(vector.payload)).toBe(vector.signature);
  const remoteSigner = remote();
  const payloadSigner = vi.fn(async ({ payload }: { payload: string }) => ({ signature: mac(payload) }));
  for (const callbacks of [{ remoteSigner }, { payloadSigner }]) {
    const provider = createRemoteSignerProvider({ exchangeName: 'bitrue', identities: { primary: identity }, ...callbacks });
    const input = request(vector.payload);
    const output = await provider(input, { signal: new AbortController().signal });
    expect(Buffer.from(output.values[0]).toString()).toBe(key);
    expect(Buffer.from(output.values[1]).toString('hex')).toBe(vector.signature);
    expect(output.receipt).toEqual(input.receipt);
  }
  expect(remoteSigner.sign).toHaveBeenCalledWith({ id: expect.any(String), type: 'ws-auth', exchangeName: 'bitrue',
    wsAuth: { toSign: vector.payload, outputFormat: 'hex' } });
  expect(payloadSigner).toHaveBeenCalledWith({ exchangeName: 'bitrue', key, payload: vector.payload, scheme: 'hmac-sha256', encoding: 'hex' });
});

it.each(vectors)('Bitrue $label: browser hosted HTTP preserves the native preimage and wire bytes', async (vector) => {
  const hostedSigning = vi.fn(async (input: HostedSigningInput) => {
    if (input.kind === 'credential') return { credential: key };
    if (input.kind !== 'http') throw new Error('Expected fixture HTTP signing request');
    return { credential: key, signature: mac(input.payload) };
  });
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '7' }) };
  const input: RemoteSignerRequest = { id: 'fixture', type: 'http', exchangeName: 'bitrue',
    config: { method: vector.method, baseURL: 'https://fapi.bitrue.com', url: vector.url, ...(vector.body === null ? {} : { data: vector.body }), headers: {} } };
  const before = structuredClone(input);
  const output = await signHostedHttp('bitrue', { key: '', secret: '' }, true, host, input);
  expect(output.error).toBeUndefined();
  expect(output.config?.url).toBe(vector.url);
  expect(hostedSigning.mock.calls).toEqual([[{ kind: 'credential' }], [{ kind: 'http', payload: vector.payload, stampMs: stamp, revision: '7' }]]);
  expect(output.config?.headers).toMatchObject({ 'X-CH-APIKEY': key, 'X-CH-SIGN': vector.signature,
    'X-CH-TS': String(stamp), recvWindow: '20000', 'X-Referer': 'Tealstreet' });
  expect(output.config?.data).toBe(vector.body ?? undefined);
  expect(input).toEqual(before);
});

it.each([{ secondary_key: 'read' }, { secondary_key: 'read', secondary_secret: 'unused' },
  { secondary_secret: 'unused' }, { localSigner: { secondary: {} } }])('refuses unselectable hosted secondary custody before callbacks: %j', (extra) => {
  const remoteSigner = remote();
  expect(() => new DelegatedSignerSetup('bitrue', { key, secret: '', remoteSigner, ...extra }))
    .toThrowError(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent', message: expect.stringContaining('Bitrue secondary read keys') }));
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

it('admits explicitly public Bitrue accounts without touching secondary custody or signer callbacks', () => {
  const remoteSigner = remote();
  const isAvailable = vi.spyOn(remoteSigner, 'isAvailable');
  const onAvailabilityChange = vi.spyOn(remoteSigner, 'onAvailabilityChange');
  // restored by #3502; #3317 dropped a live venue without a product decision.
  const exchange = new RustEngineExchange('bitrue', {
    key, secret: '', remoteSigner, secondary_key: 'read', publicDataOnly: true,
  }, new DefaultStore());
  exchange.dispose();
  expect(remoteSigner.sign).not.toHaveBeenCalled();
  expect(isAvailable).not.toHaveBeenCalled();
  expect(onAvailabilityChange).not.toHaveBeenCalled();
});

it('still refuses keyed private Bitrue accounts with the same secondary key before callbacks', () => {
  const remoteSigner = remote();
  // restored by #3502; #3317 dropped a live venue without a product decision.
  expect(() => new RustEngineExchange('bitrue', {
    key, secret: '', remoteSigner, secondary_key: 'read', publicDataOnly: false,
  }, new DefaultStore()))
    .toThrowError(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent', message: expect.stringContaining('Bitrue secondary read keys') }));
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

it('admits PayloadSigner secondary selection and primary-only RemoteSigner custody', () => {
  for (const extra of [{ remoteSigner: remote() }, { payloadSigner: async () => ({ signature: 'ab'.repeat(32) }), secondary_key: 'read' }]) {
    const setup = new DelegatedSignerSetup('bitrue', { key, secret: '', ...extra });
    expect(setup.primaryComplete).toBe(true);
    expect(setup.hasSecondary).toBe('secondary_key' in extra);
    setup.dispose();
  }
});

it('selects the explicit secondary public key for PayloadSigner work', async () => {
  const secondary = { credential: 'owned/read/public', signing: 'owned/read/hmac', publicCredential: 'read-public' };
  const payloadSigner = vi.fn(async () => ({ signature: mac('secondary-canonical-payload') }));
  const provider = createRemoteSignerProvider({ exchangeName: 'bitrue', identities: { primary: identity, secondary }, payloadSigner });
  const input = request('secondary-canonical-payload');
  input.operations[0] = { kind: 'present', key: secondary.credential };
  const sign = input.operations[1];
  if (sign.kind !== 'sign') throw new Error('fixture');
  sign.key = secondary.signing;
  const output = await provider(input, { signal: new AbortController().signal });
  expect(payloadSigner).toHaveBeenCalledWith(expect.objectContaining({ key: 'read-public', payload: 'secondary-canonical-payload' }));
  expect(Buffer.from(output.values[0]).toString()).toBe('read-public');
});

it('preserves encoded queries, UTF-8 JSON and integer protocol stamps without mutating input', () => {
  const config: InternalAxiosRequestConfig = { url: '/fapi/v2/order?fixed=a%2Bb', method: 'post', params: { symbol: 'E-BTC-USDT', tag: 'é +' },
    data: { note: 'é +', size: 0 }, headers: new AxiosHeaders() };
  const before = { ...structuredClone(config), headers: new AxiosHeaders(config.headers) };
  expect(prepareBitrueRequest(config, stamp + 0.9).payload).toBe(`${stamp}POST/fapi/v2/order?fixed=a%2Bb&symbol=E-BTC-USDT&tag=%C3%A9%20%2B{"note":"é +","size":0}`);
  expect(config).toEqual(before);
});

it('never falls back or leaks callback error text after a failed remote signature', async () => {
  const payloadSigner = vi.fn();
  const remoteSigner = remote(async () => { throw new Error('fixture-sensitive-callback-text'); });
  const provider = createRemoteSignerProvider({ exchangeName: 'bitrue', identities: { primary: identity }, remoteSigner, payloadSigner });
  await expect(provider(request('canonical-bytes'), { signal: new AbortController().signal }))
    .rejects.toThrow(/^Remote signing request refused$/);
  expect(payloadSigner).not.toHaveBeenCalled();
});
