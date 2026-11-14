import type { EngineHost, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, expect, it, vi } from 'vitest';

import { assertBetaAccount, assertBetaVenue } from '../../exchanges/rust/betaVenues';
import { deriveBridgeConstruction, deriveRemoteSigningPolicy, deriveSignerSetupPolicy } from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { signHostedHttp, signHostedWsAuth } from '../../exchanges/rust/legacySigning';
import { LocalSignerSetup } from '../../exchanges/rust/localSignerSetup';
import { signHttpCapture } from '../../utils/signHttpCapture';
import { virtualClock } from '../../utils/virtualClock';
import { createAPI } from '../reference/woo/woo.api';
import { request, vectors } from '../support/wooHostedSigning';

const stamp = 1_700_000_000_000;
const secret = 'fixture-signing-material';
const mac = (payload: string) => createHmac('sha256', secret).update(payload).digest('hex');
const owned = () => {
  const hostedSigning = vi.fn(async (input: { kind: string; payload?: string; toSign?: string }) =>
    input.kind === 'credential' ? { credential: 'fixture-public' }
      : { credential: 'fixture-public', signature: mac(input.payload ?? input.toSign!) });
  const host = { hostedSigning, hostedSigningClock: () => ({ stampMs: stamp, revision: '1' }) } as unknown as EngineHost;
  return { host, hostedSigning };
};
afterEach(() => vi.restoreAllMocks());

it.each(vectors)('Woo $label: hosted payload and MAC match the local-key wire and independent vector', async (vector) => {
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(stamp);
  const input = request(vector); const before = structuredClone(input);
  const { host, hostedSigning } = owned();
  const delegated = await signHostedHttp('woo', { key: '', secret: '' }, true, host, input);
  expect(delegated.error).toBeUndefined();
  expect(hostedSigning.mock.calls).toEqual([[{ kind: 'credential' }], [{ kind: 'http', payload: vector.payload, stampMs: stamp, revision: '1' }]]);
  const localInput = request(vector);
  localInput.config!.headers = { 'X-Transport': 'keep' };
  const local = await signHttpCapture(createAPI({ key: 'fixture-public', secret } as ExchangeOptions,
    { name: 'woo', grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: { emit: () => undefined } } as never), localInput);
  expect(local.error).toBeUndefined();
  expect(delegated.config?.headers['x-api-key']).toBe('fixture-public');
  expect(delegated.config?.headers['x-api-signature']).toBe(vector.signature);
  expect(delegated.config?.headers['x-api-signature']).toBe(local.config?.headers['x-api-signature']);
  expect(String(delegated.config?.headers['x-api-timestamp'])).toBe(String(local.config?.headers['x-api-timestamp']));
  expect(delegated.config?.url).toBe(local.config?.url);
  expect(delegated.config?.params).toEqual(local.config?.params);
  expect(delegated.config?.data).toBe(local.config?.data);
  expect(input).toEqual(before);
});

it('admits Woo RemoteSigner through its declaration, while an undeclared venue stays refused', () => {
  const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
  expect(deriveRemoteSigningPolicy('woo')?.enabled).toBe(true);
  expect(deriveBridgeConstruction('woo', {} as ExchangeOptions).dispatchSigning).toBe(true);
  expect(() => assertBetaAccount('woo', { key: 'fixture-public', secret: '', remoteSigner })).not.toThrow();
  expect(() => assertBetaVenue('mode')).not.toThrow();
  expect(deriveRemoteSigningPolicy('mode')).toBeUndefined();
  expect(() => assertBetaAccount('mode', { key: 'fixture-public', secret: '', remoteSigner })).toThrow(/isn't available on the Hosted Server/);
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

it.each([
  { secondary_key: 'fixture-read' },
  { secondary_key: 'fixture-read', secondary_secret: 'fixture-read-material' },
  { secondary_secret: 'fixture-read-material' },
  { secondary_passphrase: 'fixture-read-passphrase' },
  { localSigner: { secondary: {} } },
])('refuses private WOO secondary RemoteSigner setup before native construction or callbacks: %j', (secondary) => {
  const remoteSigner = { sign: vi.fn(), isAvailable: vi.fn(() => true), onAvailabilityChange: vi.fn(() => () => {}) };
  const Constructor = vi.fn();
  let setup: DelegatedSignerSetup | undefined;
  try {
    expect(() => {
      setup = new DelegatedSignerSetup('woo', { key: 'fixture-public', secret: '', remoteSigner, ...secondary });
      setup.construct(Constructor as unknown as new (venue: string, options: string) => WasmEngineLike, 'woo', '{}');
    }).toThrowError(expect.objectContaining({
      kind: 'unsupported', outcome: 'notSent', message: expect.stringContaining('WOO X secondary read keys'),
    }));
    expect(Constructor).not.toHaveBeenCalled();
    expect(remoteSigner.sign).not.toHaveBeenCalled();
    expect(remoteSigner.isAvailable).not.toHaveBeenCalled();
    expect(remoteSigner.onAvailabilityChange).not.toHaveBeenCalled();
  } finally {
    setup?.dispose();
  }
});

it('preserves WOO primary remote setup, local read keys and slot-aware hosted families', () => {
  const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const options = { key: 'fixture-public', secret: 'fixture-material', secondary_key: 'fixture-read', secondary_secret: 'fixture-read-material' };
  const local = new LocalSignerSetup(options, deriveSignerSetupPolicy('woo'));
  expect(local.hasSecondary).toBe(true);
  expect(local.primaryComplete).toBe(true);
  local.dispose();
  for (const [name, secondary] of [['woo', {}], ['kucoin', { secondary_key: 'fixture-read' }], ['backpack', { secondary_key: 'fixture-read' }]] as const) {
    const setup = new DelegatedSignerSetup(name, { key: 'fixture-public', secret: '', remoteSigner, ...secondary });
    expect(setup.primaryComplete).toBe(true);
    expect(setup.hasSecondary).toBe(name !== 'woo');
    setup.dispose();
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

it('delegates the exact private WS auth preimage as a hex HMAC', async () => {
  const { host, hostedSigning } = owned();
  const toSign = `|${stamp}`;
  await expect(signHostedWsAuth(host, { id: 'ws', type: 'ws-auth', exchangeName: 'woo', wsAuth: { toSign, outputFormat: 'hex' } }))
    .resolves.toEqual({ id: 'ws', wsAuthSignature: mac(toSign) });
  expect(hostedSigning).toHaveBeenCalledWith({ kind: 'wsAuth', toSign, outputFormat: 'hex' });
});

it.each([vectors[8], vectors[10]])('replayed serialized $label bytes are signed once without reserialization', async (vector) => {
  const input = request(vector);
  input.config!.data = vector.path.includes('/v1/') ? vector.payload.split('|')[0] : vector.body;
  const { host } = owned();
  const result = await signHostedHttp('woo', { key: '', secret: '' }, true, host, input);
  expect(result.config?.headers['x-api-signature']).toBe(vector.signature);
  expect(result.config?.data).toBe(input.config!.data);
});

it('keeps public reads unsigned and refuses a changed presented identity', async () => {
  const { host, hostedSigning } = owned();
  const input: RemoteSignerRequest = { id: 'public', type: 'http', exchangeName: 'woo', config: {
    method: 'GET', url: '/v1/public/info', baseURL: 'https://api.woox.io', headers: {},
  } };
  const result = await signHostedHttp('woo', { key: '', secret: '' }, true, host, input);
  expect(result.config?.headers).not.toHaveProperty('x-api-signature');
  expect(hostedSigning).not.toHaveBeenCalled();
  hostedSigning.mockImplementation(async (work) => work.kind === 'credential'
    ? { credential: 'fixture-public' } : { credential: 'rotated-public', signature: 'mac' });
  await expect(signHostedHttp('woo', { key: '', secret: '' }, true, host, request(vectors[8])))
    .rejects.toThrow('Signing identity changed');
});

it('every private route used by Woo has a signing fixture', () => {
  const paths = new Set<string>();
  for (const file of ['account.rs', 'history.rs', 'identity.rs', 'trading.rs', 'seed_read.rs']) {
    const source = readFileSync(resolve(process.cwd(), '../../sdk-rs/venues/woo/src', file), 'utf8');
    for (const match of source.matchAll(/"(\/v[13]\/[^"\s]+)"/g)) {
      if (!match[1].includes('/public/')) paths.add(match[1]);
    }
  }
  expect(paths.size).toBeGreaterThan(10);
  for (const path of paths) {
    const pattern = new RegExp(`^${path.replace(/\{[^}]*\}/g, '[^/]+')}$`);
    expect(vectors.some((vector) => pattern.test(vector.path)), path).toBe(true);
  }
});

it.each([false, true])('preserves legacy broker insertion before signing (testnet=%s)', async (testnet) => {
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(stamp);
  for (const vector of [vectors[8], vectors[10]]) {
    const input = request(vector);
    const field = vector.path.includes('/v1/') ? 'broker_id' : 'brokerId';
    delete (input.config!.data as Record<string, unknown>)[field];
    const before = structuredClone(input);
    const localInput = structuredClone(input); localInput.config!.headers = {};
    const local = await signHttpCapture(createAPI({ key: 'fixture-public', secret, testnet } as ExchangeOptions,
      { name: 'woo', grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: { emit: () => undefined } } as never), localInput);
    const { host } = owned();
    const delegated = await signHostedHttp('woo', { key: '', secret: '', testnet }, true, host, input);
    expect(delegated.config?.headers['x-api-signature']).toBe(local.config?.headers['x-api-signature']);
    expect(delegated.config?.data).toBe(local.config?.data);
    expect(input).toEqual(before);
  }
});
