import type { SignRequest, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, PayloadSigner, RemoteSigner } from '../../types';

import { expect, it, vi } from 'vitest';

import { BETA_VENUES } from '../../exchanges/rust/betaVenues';
import { exchangeMetadata } from '../../exchangeMetadata';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import rules from '../../exchanges/rust/construction-rules.json';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { LocalSignerSetup } from '../../exchanges/rust/localSignerSetup';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const venues = BETA_VENUES.filter((name) => deriveRemoteSigningPolicy(name)?.enabled);
const publicCases: Array<{ name: ExchangeName; version?: 'v1' | 'v3' }> = venues.map((name) => ({ name }));
publicCases.push({ name: 'asterdex', version: 'v1' }, { name: 'asterdex', version: 'v3' });
const unusedSigners = () => ({
  remoteSigner: {
    sign: vi.fn(), isAvailable: vi.fn(), onAvailabilityChange: vi.fn(),
  },
  payloadSigner: vi.fn(),
});

it.each<{ key?: string; publicDataOnly?: boolean }>([
  {}, { key: '' }, { publicDataOnly: false }, { key: '', publicDataOnly: false },
])('keyless AsterDEX V3 PayloadSigner resolves public mode for %j', (input) => {
  const payloadSigner = vi.fn();
  const options: ExchangeOptions = { key: '', secret: '', payloadSigner, extra: { asterdexApiVersion: 'v3' }, ...input };
  if (input.key === undefined) Reflect.deleteProperty(options, 'key');
  const setup = new DelegatedSignerSetup('asterdex', options);
  try {
    expect(setup.primaryComplete).toBe(false);
    expect(setup.credentialPresence).toEqual({ key: false, secret: false });
    const exchange = new RustEngineExchange('asterdex', options, new DefaultStore());
    try {
      expect(exchange.isPublicDataOnly).toBe(true);
      expect(exchange.privateWebsocket).toBeUndefined();
      expect(payloadSigner).not.toHaveBeenCalled();
    } finally { exchange.dispose(); }
  } finally { setup.dispose(); }
});

it('private AsterDEX V3 PayloadSigner with a wallet key retains its typed custody refusal', () => {
  const payloadSigner = vi.fn();
  expect(() => new RustEngineExchange('asterdex', { key: `0x${'11'.repeat(20)}`, secret: '', payloadSigner,
    extra: { asterdexApiVersion: 'v3' } }, new DefaultStore())).toThrowError(expect.objectContaining({
    kind: 'unsupported', outcome: 'notSent',
    message: 'AsterDEX V3 requires RemoteSigner API-wallet custody; PayloadSigner supports V1 HMAC only',
  }));
  expect(payloadSigner).not.toHaveBeenCalled();
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

type VenueCase = { name: ExchangeName; label: string; version?: 'v1' | 'v3' };
const matrixVenues = BETA_VENUES.flatMap((name): VenueCase[] => name === 'asterdex'
  ? [{ name, label: 'asterdex-v1', version: 'v1' }, { name, label: 'asterdex-v3', version: 'v3' }]
  : [{ name, label: name }]);
const publicModes: Array<{ label: string; publicDataOnly?: boolean; omitKey?: boolean }> = [
  { label: 'explicit', publicDataOnly: true }, { label: 'keyless-empty' },
  { label: 'keyless-absent', omitKey: true }, { label: 'keyless-false', publicDataOnly: false },
];
// A keyless account whose callback alone satisfies the venue's declared canSign and
// privateEligible rules is private, in TS and in the Rust registry (same rules file).
const callbackCredentials: Partial<Record<ExchangeName, 'remote' | 'payload'>> = { hyperliquidspot: 'remote' };
const isExchangeName = (name: string): name is ExchangeName => Object.hasOwn(exchangeMetadata, name);
const outsideBeta = Object.keys(rules.venues).filter(isExchangeName)
  .filter((name) => !BETA_VENUES.some((venue) => venue === name));
const privateRefusal = ({ name, version }: VenueCase, kind: 'remote' | 'payload'): string | undefined => {
  const policy = deriveRemoteSigningPolicy(name);
  if (policy?.enabled !== true) return kind === 'remote'
    ? `${exchangeMetadata[name].prettyName} isn't available on the Hosted Server yet; turn Hosted Server off to trade it with your API key in this browser`
    : `${exchangeMetadata[name].prettyName} delegated signing is unsupported in this build`;
  if (kind === 'remote') return undefined;
  if (name === 'asterdex' && version === 'v3') return 'AsterDEX V3 requires RemoteSigner API-wallet custody; PayloadSigner supports V1 HMAC only';
  if (policy.payloadSupported === true) return undefined;
  if (name === 'apexomni') return 'ApeX Omni requires RemoteSigner HMAC and zkLink contract custody';
  if (name === 'kraken') return 'Kraken Futures requires RemoteSigner SHA256/HMAC-SHA512 custody; PayloadSigner is unsupported';
  if (name === 'grvt') return 'GRVT PayloadSigner is unsupported';
  if (name === 'kucoin' || name === 'kucoinspot') return 'KuCoin requires RemoteSigner batch custody for request, passphrase and optional broker MACs';
  if (name === 'backpack') return 'Backpack requires RemoteSigner Ed25519 custody';
  return 'This Rust account has no REST-only PayloadSigner lifecycle';
};

it.each(matrixVenues)('$label resolves the complete public matrix before custody and preserves private controls', async (venue) => {
  const { name, version } = venue;
  const { remoteSigner, payloadSigner } = unusedSigners();
  const callbacks: Array<{ kind: 'remote' | 'payload'; options: Partial<ExchangeOptions> }> = [
    { kind: 'remote', options: { remoteSigner } }, { kind: 'payload', options: { payloadSigner } },
  ];
  const results = new Map<string, boolean>();
  for (const mode of publicModes) {
    let passed = true;
    for (const callback of callbacks) {
      const label = `${venue.label} ${mode.label} ${callback.kind}`;
      const options: ExchangeOptions = { key: '', secret: '', extra: { asterdexApiVersion: version },
        ...callback.options, ...(mode.publicDataOnly === undefined ? {} : { publicDataOnly: mode.publicDataOnly }) };
      if (mode.omitKey) Reflect.deleteProperty(options, 'key');
      if (mode.publicDataOnly !== true && callbackCredentials[name] === callback.kind) {
        const exchange = new RustEngineExchange(name, options, new DefaultStore());
        try { expect(exchange.isPublicDataOnly, label).toBe(false); }
        finally { exchange.dispose(); }
        continue;
      }
      let exchange: RustEngineExchange | undefined;
      try { exchange = new RustEngineExchange(name, options, new DefaultStore()); }
      catch (error) { passed = false; expect.soft(error, label).toBeUndefined(); }
      if (exchange) {
        try {
          passed = passed && exchange.isPublicDataOnly;
          expect.soft(exchange.isPublicDataOnly, label).toBe(true);
          expect.soft(exchange.privateWebsocket, label).toBeUndefined();
          const setup: unknown = Reflect.get(exchange, 'signerSetup');
          if (setup instanceof LocalSignerSetup || setup instanceof DelegatedSignerSetup)
            expect.soft(setup.primaryComplete, label).toBe(false);
          if (setup instanceof DelegatedSignerSetup) {
            const noAuthority = setup.primaryComplete === false && setup.credentialPresence.key === false
              && setup.credentialPresence.secret === false && setup.signerAlgorithms.length === 0;
            passed = passed && noAuthority;
            expect.soft(noAuthority, label).toBe(true);
            await expect.soft(setup.signer({ receipt: {}, context: { venue: name, method: 'GET', host: 'fixture.invalid', path: '/' },
              operations: [] }, { signal: new AbortController().signal }), label)
              .rejects.toMatchObject({ kind: 'unsupported', outcome: 'notSent', message: 'Public-only accounts cannot sign' });
          }
        } finally { exchange.dispose(); }
      }
    }
    results.set(mode.label, passed);
  }
  let privateControl = true;
  for (const callback of callbacks) {
    // Aster V3 resolves private only with a complete (synthetic) API-wallet context.
    const key = name === 'coinbase' || name === 'coinbasespot'
      ? ['organizations', 'fixture', 'apiKeys', 'fixture'].join('/') : `0x${'11'.repeat(20)}`;
    const options: ExchangeOptions = { key,
      secret: version === 'v3' ? `0x${'33'.repeat(32)}` : 'unused-private-material',
      passphrase: 'fixture-passphrase', apiKeyIndex: '2', applicationId: `0x${'22'.repeat(20)}`,
      paradexAddress: 'fixture-paradex-account', subkeyAddress: 'fixture-paradex-subkey',
      extra: { asterdexApiVersion: version }, ...callback.options };
    const refusal = privateRefusal(venue, callback.kind);
    let outcome: unknown;
    try {
      const exchange = new RustEngineExchange(name, options, new DefaultStore());
      outcome = exchange.isPublicDataOnly ? 'public' : 'private';
      exchange.dispose();
    } catch (error) { outcome = error; }
    const expected = refusal ? expect.objectContaining({ kind: 'unsupported', outcome: 'notSent', message: refusal }) : 'private';
    expect.soft(outcome, `${venue.label} private ${callback.kind}`).toEqual(expected);
    privateControl &&= refusal ? outcome instanceof Error && outcome.message === refusal : outcome === 'private';
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
  expect(payloadSigner).not.toHaveBeenCalled();
  const keyless = publicModes.filter(({ label }) => label !== 'explicit').every(({ label }) => results.get(label));
  // The unit setup mocks console; stdout keeps the matrix visible in CI logs.
  process.stdout.write(`MATRIX-TS ${venue.label}: ts-explicit=${results.get('explicit') ? 'pass' : 'fail'} ts-keyless=${keyless ? 'pass' : 'fail'} private-control=${privateControl ? 'pass' : 'fail'}\n`);
});

it('enumerates every declared venue outside the beta roster', () => {
  // restored by #3502; #3317 dropped a live venue without a product decision.
  expect(venues).toContain('bitrue');
  // restored by #3502; #3317 dropped a live venue without a product decision.
  expect(outsideBeta.sort()).toEqual(['bingxspot', 'polymarket', 'txflow', 'weex']);
  expect(rules.venues.hyperliquidspot).toMatchObject({ canSign: [['secret', 'remoteSigner']] });
  expect(rules.venues.hyperliquidspot).not.toHaveProperty('privateEligible');
});

it.each(outsideBeta)('%s remains outside beta admission in every public form', (name) => {
  const { remoteSigner, payloadSigner } = unusedSigners();
  for (const mode of publicModes) {
    for (const callbacks of [{}, { remoteSigner }, { payloadSigner }]) {
      const options: ExchangeOptions = { key: '', secret: '', ...callbacks,
        ...(mode.publicDataOnly === undefined ? {} : { publicDataOnly: mode.publicDataOnly }) };
      if (mode.omitKey) Reflect.deleteProperty(options, 'key');
      expect(() => new RustEngineExchange(name, options, new DefaultStore())).toThrowError(expect.objectContaining({
        kind: 'unsupported', outcome: 'notSent', message: `${name} is unsupported in this build`,
      }));
    }
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
  expect(payloadSigner).not.toHaveBeenCalled();
});

it('a private Polymarket account refuses at SDK admission before custody setup', () => {
  const { remoteSigner, payloadSigner } = unusedSigners();
  for (const callbacks of [{ remoteSigner }, { payloadSigner }]) {
    expect(() => new RustEngineExchange('polymarket', { key: `0x${'44'.repeat(32)}`, secret: '', ...callbacks }, new DefaultStore()))
      .toThrowError(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent',
        message: 'polymarket is unsupported in this build' }));
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
  expect(payloadSigner).not.toHaveBeenCalled();
});

it.each(['kucoin', 'kucoinspot', 'kraken'] as const)('%s constructs explicitly public-only accounts with an unused PayloadSigner', (name) => {
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

it.each(publicCases)('$name $version public-only shared setup ignores signing configuration and confers no authority', async ({ name, version }) => {
  const { remoteSigner, payloadSigner } = unusedSigners();
  for (const callbacks of [{ payloadSigner }, { remoteSigner }, { remoteSigner, payloadSigner },
    { remoteSigner: { sign: remoteSigner.sign } }, { payloadSigner: 'unused-invalid-callback' },
    { remoteSigner: null, payloadSigner: 'unused-invalid-callback' }]) {
    const input = {
      key: 'unused-key', secret: 'unused-material', publicDataOnly: true,
      secondary_secret: 'unused-read-material', secondary_passphrase: '', localSigner: null,
      extra: { asterdexApiVersion: version },
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

it('TxFlow inferred and explicit public setup ignores unused PayloadSigner and metadata', () => {
  const payloadSigner = vi.fn();
  for (const publicDataOnly of [undefined, true]) {
    const setup = new DelegatedSignerSetup('txflow', {
      key: '', secret: '', testnet: true, publicDataOnly, payloadSigner,
      applicationId: publicDataOnly ? '0x1111111111111111111111111111111111111111' : undefined,
      localSigner: { primary: { stableId: '' } },
    });
    try {
      expect(setup.usesPayloadSigner).toBe(false);
      expect(setup.primaryComplete).toBe(false);
      expect(setup.signerAlgorithms).toEqual([]);
    } finally { setup.dispose(); }
  }
  expect(payloadSigner).not.toHaveBeenCalled();
});

// Keys are normalized exactly as signer setup presents them to derivePublicMode:
// declared or caller trimCredential with Rust White_Space semantics.
const blankKeys = ['   ', '\u0085 '];
const callerTrimVenues: ExchangeName[] = ['kucoin', 'kucoinspot', 'grvt', 'apexomni', 'backpack', 'binance'];
const trimCases: Array<{ name: ExchangeName; localSigner?: ExchangeOptions['localSigner'] }> = [
  { name: 'bitunix' }, { name: 'mexc' },
  ...callerTrimVenues.map((name) => ({ name, localSigner: { primary: { trimCredential: true } } })),
];

it.each(trimCases)('$name resolves a key that normalizes to empty as public before custody', async ({ name, localSigner }) => {
  const { remoteSigner, payloadSigner } = unusedSigners();
  for (const key of blankKeys) {
    for (const callbacks of [{ remoteSigner }, { payloadSigner }]) {
      const label = `${name} ${JSON.stringify(key)} ${Object.keys(callbacks)[0]}`;
      const exchange = new RustEngineExchange(name, { key, secret: '', ...(localSigner ? { localSigner } : {}), ...callbacks },
        new DefaultStore());
      try {
        expect(exchange.isPublicDataOnly, label).toBe(true);
        expect(exchange.privateWebsocket, label).toBeUndefined();
        const setup: unknown = Reflect.get(exchange, 'signerSetup');
        if (setup instanceof DelegatedSignerSetup) {
          expect(setup.credentialPresence, label).toEqual({ key: false, secret: false });
          expect(setup.signerAlgorithms, label).toEqual([]);
        }
      } finally { exchange.dispose(); }
    }
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
  expect(payloadSigner).not.toHaveBeenCalled();
});

it('keeps an untrimmed whitespace key as exact private bytes when no trim policy applies', () => {
  const { remoteSigner } = unusedSigners();
  const exchange = new RustEngineExchange('binance', { key: blankKeys[0], secret: '', remoteSigner }, new DefaultStore());
  try { expect(exchange.isPublicDataOnly).toBe(false); }
  finally { exchange.dispose(); }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

const malformedCallbacks: Array<{ field: 'remoteSigner' | 'payloadSigner'; value: unknown }> = [
  { field: 'remoteSigner', value: null }, { field: 'remoteSigner', value: false },
  { field: 'remoteSigner', value: {} }, { field: 'remoteSigner', value: 'not-a-signer' },
  { field: 'payloadSigner', value: null }, { field: 'payloadSigner', value: '' },
  { field: 'payloadSigner', value: false }, { field: 'payloadSigner', value: 'not-a-function' },
];

it.each(malformedCallbacks)('a keyed account with a configured malformed $field ($value) keeps its typed refusal', ({ field, value }) => {
  const options: ExchangeOptions = { key: 'fixture-key', secret: '' };
  Reflect.set(options, field, value);
  expect(() => new RustEngineExchange('binance', options, new DefaultStore()))
    .toThrowError(expect.objectContaining({ kind: 'invalidInput', outcome: 'notSent' }));
  const keyless: ExchangeOptions = { key: '', secret: '' };
  Reflect.set(keyless, field, value);
  const exchange = new RustEngineExchange('binance', keyless, new DefaultStore());
  try { expect(exchange.isPublicDataOnly).toBe(true); }
  finally { exchange.dispose(); }
});

// Venues whose delegated signing is disabled never reach DelegatedSignerSetup, so beta
// admission itself must refuse a configured-but-malformed callback on a keyed account.
const disabledDelegation = matrixVenues.filter(({ name }) => deriveRemoteSigningPolicy(name)?.enabled !== true);
const callbackFields: Array<'remoteSigner' | 'payloadSigner'> = ['remoteSigner', 'payloadSigner'];
const falsyCallbacks = callbackFields.flatMap((field) => [null, false, ''].map((value) => ({ field, value })));

it('covers every disabled-delegation beta venue', () => {
  expect(venues).toContain('cryptocom');
  expect(venues).toContain('asterdex');
  expect(venues).toContain('mexc');
  expect(disabledDelegation.map(({ label }) => label).sort()).toEqual([
    'kodiakfi', 'krakenspot',
    'mode', 'what', 'woofi',
  ].sort());
});

it.each(disabledDelegation)('$label refuses a keyed account with a configured falsy callback as invalid input', ({ name, version }) => {
  for (const { field, value } of falsyCallbacks) {
    const label = `${name} ${version ?? ''} ${field}=${JSON.stringify(value)}`;
    const keyed: ExchangeOptions = { key: 'fixture-key', secret: '', extra: { asterdexApiVersion: version } };
    Reflect.set(keyed, field, value);
    let outcome: unknown;
    try {
      const exchange = new RustEngineExchange(name, keyed, new DefaultStore());
      outcome = exchange.isPublicDataOnly ? 'public' : 'private';
      exchange.dispose();
    } catch (error) { outcome = error; }
    // A present-but-invalid callback on a keyed account refuses before mode resolution, even
    // where the rules would resolve public (Aster V3 without RemoteSigner or wallet context).
    expect.soft(outcome, label).toEqual(expect.objectContaining({ kind: 'invalidInput', outcome: 'notSent' }));
    const explicit: ExchangeOptions = { ...keyed, publicDataOnly: true };
    const publicExchange = new RustEngineExchange(name, explicit, new DefaultStore());
    try { expect.soft(publicExchange.isPublicDataOnly, `${label} explicit`).toBe(true); }
    finally { publicExchange.dispose(); }
    const keyless: ExchangeOptions = { key: '', secret: '', extra: { asterdexApiVersion: version } };
    Reflect.set(keyless, field, value);
    const exchange = new RustEngineExchange(name, keyless, new DefaultStore());
    try { expect.soft(exchange.isPublicDataOnly, `${label} keyless`).toBe(true); }
    finally { exchange.dispose(); }
  }
});

it('keeps the review controls for configured callbacks', () => {
  const { remoteSigner } = unusedSigners();
  const binance: ExchangeOptions = { key: 'fixture-key', secret: '' };
  Reflect.set(binance, 'remoteSigner', null);
  expect(() => new RustEngineExchange('binance', binance, new DefaultStore()))
    .toThrowError(expect.objectContaining({ kind: 'invalidInput', outcome: 'notSent' }));
  expect(() => new RustEngineExchange('krakenspot', { key: 'fixture-key', secret: '', remoteSigner }, new DefaultStore()))
    .toThrowError(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent',
      message: `${exchangeMetadata.krakenspot.prettyName} isn't available on the Hosted Server yet; turn Hosted Server off to trade it with your API key in this browser` }));
  const privateKraken = new RustEngineExchange('kraken', { key: 'fixture-key', secret: '', remoteSigner }, new DefaultStore());
  try { expect(privateKraken.isPublicDataOnly).toBe(false); }
  finally { privateKraken.dispose(); }
  const keylessKraken: ExchangeOptions = { key: '', secret: '' };
  Reflect.set(keylessKraken, 'remoteSigner', null);
  const exchange = new RustEngineExchange('kraken', keylessKraken, new DefaultStore());
  try { expect(exchange.isPublicDataOnly).toBe(true); }
  finally { exchange.dispose(); }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

const inspectionMarker = 'SYNTHETIC_PRIVATE_CALLBACK_MARKER';
const hostileCallbacks: Array<{
  field: 'remoteSigner' | 'payloadSigner'; shape: string; create: () => unknown;
}> = callbackFields.flatMap((field): Array<{
  field: 'remoteSigner' | 'payloadSigner'; shape: string; create: () => unknown;
}> => {
  const target = (): RemoteSigner | PayloadSigner => field === 'remoteSigner'
    ? unusedSigners().remoteSigner : unusedSigners().payloadSigner;
  const fail = () => { throw new Error(inspectionMarker); };
  return [
    { field, shape: 'throwing getter', create: () => Object.defineProperty(target(),
      field === 'remoteSigner' ? 'sign' : 'call', { get: fail }) },
    { field, shape: 'revoked Proxy', create: () => {
      const proxy = Proxy.revocable(target(), {});
      proxy.revoke();
      return proxy.proxy;
    } },
    { field, shape: 'throwing has trap', create: () => new Proxy(target(), { has: fail }) },
    { field, shape: 'throwing get trap', create: () => new Proxy(target(), { get: fail }) },
  ];
});

it.each(hostileCallbacks)('redacts $field inspection failures from a $shape before transport', ({ field, create }) => {
  const request = vi.fn();
  const connect = vi.fn();
  const names: ExchangeName[] = ['kraken', 'binance'];
  for (const name of names) {
    const options: ExchangeOptions = { key: 'fixture-key', secret: '',
      transport: { name: 'callback-inspection-probe', rest: () => { request(); return { proxied: false }; },
        restAdapter: () => undefined, proxyPrefix: () => '', publicCorsProxy: () => undefined,
        privateProxy: () => undefined, serverProxyUrl: () => undefined, requireProxy: (value) => value,
        wsRouting: () => ({ restProxyHost: '', howEgress: 'sg', wsPublic: { tunnelled: false },
          wsPrivate: { tunnelled: false }, wsBusiness: { tunnelled: false } }),
        wsConnection: () => { connect(); return undefined; } } };
    Reflect.set(options, field, create());
    let error: unknown;
    let exchange: RustEngineExchange | undefined;
    try { exchange = new RustEngineExchange(name, options, new DefaultStore()); }
    catch (caught) { error = caught; }
    try {
      expect.soft(error, `${name} ${field}`).toMatchObject({ kind: 'invalidInput', outcome: 'notSent',
        message: 'Invalid delegated signer setup' });
      expect.soft(String(error)).not.toContain(inspectionMarker);
      expect.soft(JSON.stringify(error, Object.getOwnPropertyNames(error ?? {}))).not.toContain(inspectionMarker);
      expect.soft(error).not.toHaveProperty('cause');
      expect.soft(exchange?.privateWebsocket).toBeUndefined();
    } finally { exchange?.dispose(); }
    for (const publicOptions of [{ key: '', secret: '' }, { key: 'fixture-key', secret: '', publicDataOnly: true }]) {
      Reflect.set(publicOptions, field, create());
      const publicExchange = new RustEngineExchange(name, publicOptions, new DefaultStore());
      try {
        expect(publicExchange.isPublicDataOnly).toBe(true);
        expect(publicExchange.privateWebsocket).toBeUndefined();
      } finally { publicExchange.dispose(); }
    }
  }
  expect(request).not.toHaveBeenCalled();
  expect(connect).not.toHaveBeenCalled();
  expect(global.fetch).not.toHaveBeenCalled();
});

it('Apex captured explicit public mode cannot be replaced with private signing input', () => {
  const setup = new DelegatedSignerSetup('apexomni', { key: '', secret: '', publicDataOnly: true, payloadSigner: vi.fn() });
  let config: Record<string, unknown> = {};
  class Engine { constructor(_venue: string, json: string) { config = JSON.parse(json); } }
  setup.construct(Engine as unknown as new (venue: string, json: string) => WasmEngineLike, 'apexomni',
    JSON.stringify({ key: 'forged', publicDataOnly: false, publicMode: { requested: false }, remoteSigning: true }));
  expect(config).toMatchObject({ publicDataOnly: true, publicMode: { requested: true }, remoteSigning: false, signing: {} });
  expect(config).not.toHaveProperty('key');
  setup.dispose();
});

it.each<ExchangeName>(['apexomni'])('%s disposed public-only setup cannot construct an engine', (name) => {
  const setup = new DelegatedSignerSetup(name, { key: '', secret: '', publicDataOnly: true });
  setup.dispose();
  const Engine = vi.fn();
  expect(() => setup.construct(Engine as unknown as new (venue: string, json: string) => WasmEngineLike, name, '{}'))
    .toThrow('disposed');
  expect(Engine).not.toHaveBeenCalled();
});
