import type { SignRequest, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { expect, it, vi } from 'vitest';

import { BETA_VENUES } from '../../exchanges/rust/betaVenues';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const venues = BETA_VENUES.filter((name) => deriveRemoteSigningPolicy(name)?.enabled);
const unusedSigners = () => ({
  remoteSigner: {
    sign: vi.fn(), isAvailable: vi.fn(), onAvailabilityChange: vi.fn(),
  },
  payloadSigner: vi.fn(),
});

it('Apex public-only construction ignores a saved key without a passphrase and an unused RemoteSigner', () => {
  const { remoteSigner } = unusedSigners();
  const exchange = new RustEngineExchange('apexomni', {
    key: 'saved-public-key', secret: '', publicDataOnly: true, remoteSigner,
  }, new DefaultStore());
  try { expect(exchange.isPublicDataOnly).toBe(true); } finally { exchange.dispose(); }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

it.each(['apexomni', 'kucoin'] as const)('%s constructs public-only accounts without validating unused signing material', (name) => {
  const { remoteSigner, payloadSigner } = unusedSigners();
  for (const key of ['', 'unused-public-key']) {
    for (const callbacks of [{ remoteSigner }, { payloadSigner }, { remoteSigner, payloadSigner }]) {
      const exchange = new RustEngineExchange(name, { key, secret: '', publicDataOnly: true, ...callbacks }, new DefaultStore());
      try {
        expect(exchange.isPublicDataOnly).toBe(true);
        expect(exchange.privateWebsocket).toBeUndefined();
        for (const field of ['key', 'secret', 'passphrase', 'payloadSigner', 'remoteSigner'])
          expect(exchange.options).not.toHaveProperty(field);
      } finally { exchange.dispose(); }
    }
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
  expect(remoteSigner.isAvailable).not.toHaveBeenCalled();
  expect(remoteSigner.onAvailabilityChange).not.toHaveBeenCalled();
  expect(payloadSigner).not.toHaveBeenCalled();
});

it.each(['apexomni', 'kucoin', 'binance', 'okx'] as const)('%s resolves explicitly public keyless accounts before validating callbacks', (name) => {
  const { remoteSigner, payloadSigner } = unusedSigners();
  for (const callbacks of [{ remoteSigner }, { payloadSigner }, { remoteSigner, payloadSigner }]) {
    const exchange = new RustEngineExchange(name, { key: '', secret: '', publicDataOnly: true, ...callbacks }, new DefaultStore());
    try { expect(exchange.isPublicDataOnly).toBe(true); } finally { exchange.dispose(); }
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
  expect(payloadSigner).not.toHaveBeenCalled();
});

it.each(['kucoin', 'kucoinspot'] as const)('%s constructs explicitly public-only accounts with an unused PayloadSigner', (name) => {
  const payloadSigner = vi.fn();
  for (const key of ['', 'unused-public-key']) {
    const exchange = new RustEngineExchange(name, { key, secret: '', publicDataOnly: true, payloadSigner }, new DefaultStore());
    try {
      expect(exchange.isPublicDataOnly).toBe(true);
      expect(exchange.privateWebsocket).toBeUndefined();
      for (const field of ['key', 'secret', 'passphrase', 'payloadSigner', 'remoteSigner'])
        expect(exchange.options).not.toHaveProperty(field);
    } finally { exchange.dispose(); }
  }
  expect(payloadSigner).not.toHaveBeenCalled();
});

it.each(venues)('%s public-only shared setup ignores signing configuration and confers no authority', async (name) => {
  const { remoteSigner, payloadSigner } = unusedSigners();
  for (const callbacks of [{ payloadSigner }, { remoteSigner }, { remoteSigner, payloadSigner },
    { remoteSigner: null, payloadSigner: 'unused-invalid-callback' }]) {
    const input = {
      key: 'unused-key', secret: 'unused-material', publicDataOnly: true,
      secondary_secret: 'unused-read-material', secondary_passphrase: '', localSigner: null,
      ...callbacks,
    } as unknown as ExchangeOptions;
    const setup = new DelegatedSignerSetup(name, input);
    try {
      let config: Record<string, unknown> = {};
      class Engine { constructor(_venue: string, json: string) { config = JSON.parse(json); } }
      setup.construct(Engine as unknown as new (venue: string, json: string) => WasmEngineLike, name,
        JSON.stringify({ ...input, publicDataOnly: false, signing: { primary: { credential: 'forged' } } }));
      expect(config).toMatchObject({
        signing: {}, publicDataOnly: true, remoteSigning: false, payloadSigner: false,
        publicMode: { requested: true, remoteSigner: callbacks.remoteSigner !== undefined,
          payloadSigner: callbacks.payloadSigner !== undefined },
      });
      for (const field of ['key', 'secret', 'secondary_secret', 'localSigner', 'remoteSigner'])
        expect(config).not.toHaveProperty(field);
      expect(setup.primaryComplete).toBe(false);
      expect(setup.hasSecondary).toBe(false);
      expect(setup.credentialPresence).toEqual({ key: false, secret: false });
      expect(setup.signerAlgorithms).toEqual([]);
      await expect(setup.signer({} as SignRequest, { signal: new AbortController().signal }))
        .rejects.toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
      expect(input.key).toBe('unused-key');
    } finally { setup.dispose(); }
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
  expect(remoteSigner.isAvailable).not.toHaveBeenCalled();
  expect(remoteSigner.onAvailabilityChange).not.toHaveBeenCalled();
  expect(payloadSigner).not.toHaveBeenCalled();
});

it('captured explicit public mode cannot be replaced with private signing input', () => {
  const setup = new DelegatedSignerSetup('apexomni', { key: '', secret: '', publicDataOnly: true, payloadSigner: vi.fn() });
  let config: Record<string, unknown> = {};
  class Engine { constructor(_venue: string, json: string) { config = JSON.parse(json); } }
  setup.construct(Engine as unknown as new (venue: string, json: string) => WasmEngineLike, 'apexomni',
    JSON.stringify({ key: 'forged', publicDataOnly: false, publicMode: { requested: false }, remoteSigning: true }));
  expect(config).toMatchObject({ publicDataOnly: true, publicMode: { requested: true }, remoteSigning: false, signing: {} });
  expect(config).not.toHaveProperty('key');
  setup.dispose();
});

it.each<ExchangeName>(['apexomni', 'kucoinspot'])('%s disposed public-only setup cannot construct an engine', (name) => {
  const setup = new DelegatedSignerSetup(name, { key: '', secret: '', publicDataOnly: true });
  setup.dispose();
  const Engine = vi.fn();
  expect(() => setup.construct(Engine as unknown as new (venue: string, json: string) => WasmEngineLike, name, '{}'))
    .toThrow('disposed');
  expect(Engine).not.toHaveBeenCalled();
});
