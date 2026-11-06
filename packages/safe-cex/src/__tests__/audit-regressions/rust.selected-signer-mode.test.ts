vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { SignRequest, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, RemoteSigner } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const aliases = ['binance', 'binancepm', 'binanceinverse', 'binancespot', 'bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'] as const;
const live: RustEngineExchange[] = [];
afterEach(() => live.splice(0).forEach((exchange) => exchange.dispose()));
const remote = (): RemoteSigner => ({
  sign: vi.fn(async ({ id }) => ({ id, wsAuthSignature: 'ab'.repeat(32) })),
  isAvailable: () => true, onAvailabilityChange: () => () => {},
});
const options = (extra: Partial<ExchangeOptions> = {}): ExchangeOptions => ({
  key: 'selected-public', secret: '', ...extra,
});
const construct = (setup: DelegatedSignerSetup, venue: ExchangeName, input = '{}') => {
  let configuration: Record<string, any> = {};
  class Engine { constructor(_venue: string, json: string) { configuration = JSON.parse(json); } }
  setup.construct(Engine as unknown as new (venue: string, json: string) => WasmEngineLike, venue, input);
  return configuration;
};
const work = (configuration: Record<string, any>): SignRequest => ({
  receipt: {}, context: { venue: 'advisory', method: 'GET', host: '', path: '/' }, operations: [{
    kind: 'sign', algorithm: 'hmacSha256', key: configuration.signing.primary.signing,
    payload: [{ kind: 'bytes', bytes: [65] }],
  }],
});

it.each(aliases)('%s selects RemoteSigner for the actual native flag while retaining both raw intent fields', async (name) => {
  const remoteSigner = remote(); const payloadSigner = vi.fn(() => ({ signature: 'cd'.repeat(32) }));
  const setup = new DelegatedSignerSetup(name, options({ remoteSigner, payloadSigner }));
  const configuration = construct(setup, name, '{"payloadSigner":true,"publicMode":{"payloadSigner":false,"remoteSigner":false}}');
  expect(configuration).toMatchObject({ remoteSigning: true, payloadSigner: false,
    publicMode: { payloadSigner: true, remoteSigner: true } });
  expect(setup).toHaveProperty('usesPayloadSigner', false);
  await setup.signer(work(configuration), { signal: new AbortController().signal });
  expect(remoteSigner.sign).toHaveBeenCalledTimes(1); expect(payloadSigner).not.toHaveBeenCalled();
  setup.dispose();
});

it.each(aliases.filter((name) => name !== 'hyrotrader'))('%s payload-only setup retains native REST-only mode', (name) => {
  const setup = new DelegatedSignerSetup(name, options({ remoteSigner: undefined, payloadSigner: () => ({ signature: 'ab'.repeat(32) }) }));
  expect(construct(setup, name)).toMatchObject({ payloadSigner: true, publicMode: { payloadSigner: true, remoteSigner: false } });
  setup.dispose();
});

it.each(['', 'unused-local-fixture'])('Hyro accepts the selected RemoteSigner with an unused PayloadSigner and local secret %j', (secret) => {
  const payloadSigner = vi.fn(() => ({ signature: 'cd'.repeat(32) }));
  expect(() => {
    const exchange = new RustEngineExchange('hyrotrader', options({ secret, remoteSigner: remote(), payloadSigner }), new DefaultStore());
    live.push(exchange); expect(exchange.isPublicDataOnly).toBe(false);
    expect(exchange.options).not.toHaveProperty('secret');
  }).not.toThrow();
  expect(payloadSigner).not.toHaveBeenCalled();
});

it.each(['', 'unused-local-fixture'])('Hyro private PayloadSigner-only remains typed unsupported with local secret %j', (secret) => {
  let error: unknown;
  try { new RustEngineExchange('hyrotrader', options({ secret, payloadSigner: () => ({ signature: 'ab'.repeat(32) }) }), new DefaultStore()); } catch (value) { error = value; }
  expect(error).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
});

it.each([null, false, ''])('an explicitly malformed RemoteSigner %j cannot fall back to PayloadSigner', (remoteSigner) => {
  const payloadSigner = vi.fn(() => ({ signature: 'ab'.repeat(32) }));
  let error: unknown;
  try { new DelegatedSignerSetup('binance', options({ remoteSigner: remoteSigner as unknown as RemoteSigner, payloadSigner })); } catch (value) { error = value; }
  expect(error).toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
  expect(payloadSigner).not.toHaveBeenCalled();
});

it('a malformed explicitly configured unused PayloadSigner retains strict validation', () => {
  let error: unknown;
  try { new DelegatedSignerSetup('binance', options({ remoteSigner: remote(), payloadSigner: null as unknown as ExchangeOptions['payloadSigner'] })); } catch (value) { error = value; }
  expect(error).toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
});

it.each(aliases)('%s public-mode raw intent is unchanged while native mode follows the selected callback', (name) => {
  const setup = new DelegatedSignerSetup(name, options({ key: '', publicDataOnly: true,
    remoteSigner: remote(), payloadSigner: () => ({ signature: 'ab'.repeat(32) }) }));
  expect(construct(setup, name)).toMatchObject({ payloadSigner: false,
    publicMode: { requested: true, remoteSigner: true, payloadSigner: true } });
  setup.dispose();
});
