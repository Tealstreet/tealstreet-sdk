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

class CapturedEngine implements WasmEngineLike {
  readonly options: unknown;
  constructor(_venue: string, json: string) { this.options = JSON.parse(json); }
  capabilities = () => '{}';
  start = () => '[]';
  command = () => '[]';
  takeFrame = () => undefined;
  snapshotFrame = () => new Uint8Array();
  wsOpen = () => '[]';
  wsMessage = () => undefined;
  wsClose = () => '[]';
  wsRouteExhausted = () => '[]';
  httpStart = () => '[]';
  httpChunk = () => '[]';
  httpEnd = () => '[]';
  httpFailed = () => '[]';
  timer = () => '[]';
}

const bybitHyroVenues: readonly ExchangeName[] = ['bybit', 'hyrotrader'];

it.each(bybitHyroVenues)('%s explicit and keyless public modes install no delegated authority', (name) => {
  const { remoteSigner, payloadSigner } = unusedSigners();
  for (const callbacks of [{ remoteSigner }, { payloadSigner }]) {
    for (const mode of [{ key: 'unused-key', publicDataOnly: true }, { key: '' }, { key: '', publicDataOnly: false }]) {
      const options: ExchangeOptions = { secret: '', ...mode, ...callbacks };
      const setup = new DelegatedSignerSetup(name, options);
      try {
        const engine = setup.construct(CapturedEngine, name, JSON.stringify(options));
        if (!(engine instanceof CapturedEngine)) throw new Error('Expected the capture engine');
        expect(engine.options).toMatchObject({ signing: {}, remoteSigning: false, payloadSigner: false, publicDataOnly: true });
        expect(setup.signerAlgorithms).toEqual([]);
        expect(setup.primaryComplete).toBe(false);
        const exchange = new RustEngineExchange(name, options, new DefaultStore());
        try {
          expect(exchange.isPublicDataOnly).toBe(true);
          expect(exchange.privateWebsocket).toBeUndefined();
        } finally { exchange.dispose(); }
      } finally { setup.dispose(); }
    }
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
  expect(remoteSigner.isAvailable).not.toHaveBeenCalled();
  expect(remoteSigner.onAvailabilityChange).not.toHaveBeenCalled();
  expect(payloadSigner).not.toHaveBeenCalled();
});

it.each(bybitHyroVenues)('%s keyed private RemoteSigner retains its primary authority', (name) => {
  const { remoteSigner } = unusedSigners();
  const options: ExchangeOptions = { key: 'fixture-key', secret: '', remoteSigner };
  const setup = new DelegatedSignerSetup(name, options);
  try {
    const engine = setup.construct(CapturedEngine, name, JSON.stringify(options));
    if (!(engine instanceof CapturedEngine)) throw new Error('Expected the capture engine');
    expect(engine.options).toMatchObject({ remoteSigning: true, signing: { primary: {
      credential: expect.any(String), signing: expect.any(String),
    } } });
    expect(setup.primaryComplete).toBe(true);
    const exchange = new RustEngineExchange(name, options, new DefaultStore());
    try { expect(exchange.isPublicDataOnly).toBe(false); } finally { exchange.dispose(); }
  } finally { setup.dispose(); }
});

it('retains private Bybit PayloadSigner support and Hyro PayloadSigner refusal', () => {
  const options: ExchangeOptions = { key: 'fixture-key', secret: '', payloadSigner: vi.fn() };
  const exchange = new RustEngineExchange('bybit', options, new DefaultStore());
  try { expect(exchange.isPublicDataOnly).toBe(false); } finally { exchange.dispose(); }
  expect(() => new RustEngineExchange('hyrotrader', options, new DefaultStore()))
    .toThrow(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent' }));
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

it('a disposed public-only setup cannot construct an engine', () => {
  const setup = new DelegatedSignerSetup('kucoinspot', { key: '', secret: '', publicDataOnly: true });
  setup.dispose();
  const Engine = vi.fn();
  expect(() => setup.construct(Engine as unknown as new (venue: string, json: string) => WasmEngineLike, 'kucoinspot', '{}'))
    .toThrow('disposed');
  expect(Engine).not.toHaveBeenCalled();
});
