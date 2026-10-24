import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';

import { describe, expect, it } from 'vitest';

import { RustEngineExchange, runsOnRustEngine } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { LocalSignerSetup, withoutSigningCredentials } from '../../exchanges/rust/localSignerSetup';
import { deriveBridgeConstruction, derivePublicMode, deriveSignerSetupPolicy } from '../../exchanges/rust/construction';

const options = () =>
  ({
    key: 'audit-secret-sentinel',
    secret: 'audit-secret-sentinel',
    secondary_key: 'primary-key',
    secondary_secret: 'audit-secret-sentinel',
    passphrase: 'audit-secret-sentinel',
    secondary_passphrase: 'audit-secret-sentinel',
    secondaryProxyUrl: 'https://proxy.invalid',
    publicDataOnly: false,
  }) as ExchangeOptions;

describe('trusted local signer setup', () => {
  it('separates credential setup from retained generic options and consumes it exactly once', () => {
    const input = options();
    const setup = new LocalSignerSetup(input);
    const safe = withoutSigningCredentials(input);
    expect(JSON.stringify(safe)).not.toContain('audit-secret-sentinel');
    expect(JSON.stringify(safe)).not.toContain('primary-key');
    expect(safe.secondaryProxyUrl).toBe(input.secondaryProxyUrl);
    expect(input.secret).toBe('audit-secret-sentinel');
    const calls: unknown[][] = [];
    class Engine {
      constructor(...args: unknown[]) {
        calls.push(args);
      }
    }
    setup.construct(
      Engine as unknown as new (venue: string, optionsJson: string, signerSetupJson?: string) => WasmEngineLike,
      'binance',
      JSON.stringify(safe),
    );
    expect(calls[0][1]).not.toContain('audit-secret-sentinel');
    expect(JSON.parse(calls[0][2] as string)).toEqual({
      primary: { key: input.key, secret: input.secret, passphrase: input.passphrase },
      secondary: { key: input.secondary_key, secret: input.secondary_secret, passphrase: input.secondary_passphrase },
    });
    expect(() => setup.construct(Engine as never, 'binance', '{}')).toThrow(/consumed/);
    expect(JSON.stringify(setup)).not.toContain('audit-secret-sentinel');
  });
  it('disposal and failed construction both release the one-use setup', () => {
    for (const fail of [false, true]) {
      const setup = new LocalSignerSetup(options());
      class Engine {
        constructor() {
          throw new Error('construction refused');
        }
      }
      if (fail) expect(() => setup.construct(Engine as never, 'binance', '{}')).toThrow('construction refused');
      else setup.dispose();
      expect(() => setup.construct(Engine as never, 'binance', '{}')).toThrow(/consumed|disposed/);
    }
  });
  it('classifies incomplete secondary setup before a generic exchange can retain it', () => {
    for (const read of [
      { secondary_key: 'primary-key' },
      { secondary_secret: 'audit-secret-sentinel' },
      { secondary_passphrase: '' },
    ])
      expect(
        () => new LocalSignerSetup({ ...withoutSigningCredentials(options()), ...read } as ExchangeOptions),
      ).toThrow(/nonempty key and secret/);
  });
  it('carries trusted metadata and an empty passphrase only through the one-use setup', () => {
    const input = { ...options(), passphrase: '', localSigner: {
      primary: { stableId: 'account/17', secretEncoding: 'base64' as const, trimCredential: true },
      secondary: { stableId: 'read/17' },
    } };
    const setup = new LocalSignerSetup(input);
    expect(withoutSigningCredentials(input)).not.toHaveProperty('localSigner');
    let received: unknown;
    class Engine {
      constructor(_venue: string, _options: string, json?: string) { received = JSON.parse(json!); }
    }
    setup.construct(Engine as never, 'binance', '{}');
    expect(received).toMatchObject({
      primary: { passphrase: '', ...input.localSigner.primary },
      secondary: { ...input.localSigner.secondary },
    });
    expect(() => setup.construct(Engine as never, 'binance', '{}')).toThrow(/consumed/);
  });
  it.each(['base64Text', 'base58'] as const)('forwards the declared %s key encoding to the Rust provider unchanged', (encoding) => {
    let received: unknown;
    class Engine {
      constructor(_venue: string, _options: string, json?: string) { received = JSON.parse(json!); }
    }
    new LocalSignerSetup({ ...options(), localSigner: { primary: { secretEncoding: encoding } } })
      .construct(Engine as never, 'binance', '{}');
    expect(received).toMatchObject({ primary: { secretEncoding: encoding } });
  });
  it('refuses provider metadata on unmigrated Rust families before construction', () => {
    expect(() => new RustEngineExchange('kraken', { ...options(), localSigner: {} }, new DefaultStore()))
      .toThrow(/not supported/);
  });
  it('rejects malformed or unknown setup metadata before constructing an engine', () => {
    let constructors = 0;
    class Engine { constructor() { constructors++; } }
    for (const localSigner of [
      null, [], 'base64', { tertiary: {} }, { primary: [] },
      { primary: { secretEncodng: 'base64' } },
      { primary: { secretEncoding: 'unknown' } },
      { primary: { trimCredential: 'true' } },
      { primary: { stableId: '' } },
      { primary: { stableId: 'bad\nidentity' } },
      { primary: { stableId: 'x'.repeat(257) } },
    ]) {
      let failure: unknown;
      try {
        new LocalSignerSetup({ ...options(), localSigner } as unknown as ExchangeOptions)
          .construct(Engine as never, 'binance', '{}');
      } catch (error) { failure = error; }
      expect(failure).toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
      expect(constructors).toBe(0);
    }
  });
});


it.each(['binance', 'binancepm', 'binanceinverse', 'binancespot', 'bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'] as const)(
  '%s retains private intent without putting credentials in generic exchange or worker options',
  (venue) => {
    const input = options();
    const exchange = new RustEngineExchange(venue, input, new DefaultStore());
    try {
      expect(exchange.isPublicDataOnly).toBe(false);
      expect(JSON.stringify(exchange.options)).not.toContain('audit-secret-sentinel');
      expect(JSON.stringify(exchange.options)).not.toContain('primary-key');
      for (const field of ['key', 'secret', 'passphrase', 'secondary_key', 'secondary_secret', 'secondary_passphrase'])
        expect(exchange.options).not.toHaveProperty(field);
      expect(input.secret).toBe('audit-secret-sentinel');
    } finally { exchange.dispose(); }
  },
);


it('incomplete secondary setup reports typed invalidInput/notSent at construction', () => {
  let failure: unknown;
  try { new LocalSignerSetup({ ...options(), secondary_secret: '' }); } catch (error) { failure = error; }
  expect(failure).toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
});


it.each([
  ['NEL is trimmed', '\u0085', true, false],
  ['BOM is preserved', '\ufeff', true, true],
  ['ASCII whitespace is trimmed', ' \t\n', true, false],
  ['interior whitespace is preserved', '  key value  ', true, true],
  ['caller preservation is retained', '\u0085', false, true],
] as const)('%s in synchronous provider intent', (_label, key, trimCredential, complete) => {
  const input = { ...options(), key, localSigner: { primary: { trimCredential } } };
  const setup = new LocalSignerSetup(input);
  expect(setup.primaryComplete).toBe(complete);
  const exchange = new RustEngineExchange('binance', input, new DefaultStore());
  try {
    expect(exchange.isPublicDataOnly).toBe(!complete);
    expect(JSON.stringify(exchange.options)).not.toContain('audit-secret-sentinel');
    expect(input.key).toBe(key);
  } finally {
    setup.dispose();
    exchange.dispose();
  }
});


describe('shared credential setup policy', () => {
  it('declares MEXC primary trimming and secondary preservation once without enabling signing', () => {
    const policy = deriveSignerSetupPolicy('mexc');
    expect(policy).toEqual({ primary: { trimCredential: true }, secondary: { trimCredential: false } });
    expect(deriveSignerSetupPolicy('binance')).toEqual({});
    const input = { ...options(), key: '\u0085 primary ', secondary_key: ' secondary ', localSigner: {
      primary: { trimCredential: false, stableId: 'primary/7' },
      secondary: { trimCredential: true, stableId: 'read/7' },
    } };
    const setup = new LocalSignerSetup(input, policy);
    let received: unknown;
    class Engine { constructor(_venue: string, _options: string, json?: string) { received = JSON.parse(json!); } }
    setup.construct(Engine as never, 'mexc', '{}');
    expect(received).toEqual({
      primary: { key: input.key, secret: input.secret, passphrase: input.passphrase, stableId: 'primary/7', trimCredential: true },
      secondary: { key: input.secondary_key, secret: input.secondary_secret, passphrase: input.secondary_passphrase, stableId: 'read/7', trimCredential: false },
    });
    expect(input.localSigner.primary.trimCredential).toBe(false);
    expect(input.localSigner.secondary.trimCredential).toBe(true);
    expect(input.key).toBe('\u0085 primary ');
    expect(() => new RustEngineExchange('mexc', input, new DefaultStore())).toThrow(/not supported/);
  });

  it.each([['\u0085', false], ['\ufeff', true], [' \t', false], ['  a b  ', true]] as const)(
    'uses native whitespace semantics for MEXC intent %j', (key, present) => {
      const input = { ...options(), key, localSigner: { primary: { trimCredential: false } } };
      const setup = new LocalSignerSetup(input, deriveSignerSetupPolicy('mexc'));
      expect(setup.primaryComplete).toBe(present);
      expect(derivePublicMode('mexc', input, undefined, setup.credentialPresence))
        .toEqual({ canSign: present, publicDataOnly: !present });
      setup.dispose();
    },
  );

  it('does not invent slots solely because a family has a setup policy', () => {
    const setup = new LocalSignerSetup({ publicDataOnly: true } as ExchangeOptions, deriveSignerSetupPolicy('mexc'));
    let received: unknown;
    class Engine { constructor(_venue: string, _options: string, json?: string) { received = JSON.parse(json!); } }
    setup.construct(Engine as never, 'mexc', '{}');
    expect(received).toEqual({});
    expect(setup.primaryComplete).toBe(false);
    expect(setup.hasSecondary).toBe(false);
  });

  it('validates secondary effective presence without trimming secrets or overriding undeclared metadata', () => {
    const input = { ...options(), secondary_key: '\u0085', localSigner: { secondary: { trimCredential: true } } };
    expect(() => new LocalSignerSetup(input)).toThrow(/nonempty key and secret/);
    const setup = new LocalSignerSetup(input, deriveSignerSetupPolicy('mexc'));
    expect(setup.hasSecondary).toBe(true);
    let received: unknown;
    class Engine { constructor(_venue: string, _options: string, json?: string) { received = JSON.parse(json!); } }
    setup.construct(Engine as never, 'mexc', '{}');
    expect(received).toMatchObject({ secondary: { key: '\u0085', secret: input.secondary_secret, passphrase: input.secondary_passphrase, trimCredential: false } });
  });
});


const passphraseFamilies = ['okx', 'okxeu', 'okxdemo', 'okxeudemo', 'bitget', 'bitgetinverse', 'bitgetusdc', 'weex'] as const;
const requiredSetup = (input: Partial<ExchangeOptions>, venue: typeof passphraseFamilies[number] = 'okx') =>
  new LocalSignerSetup(input as ExchangeOptions, deriveSignerSetupPolicy(venue));
const setupFailure = (run: () => unknown): unknown => {
  try { run(); } catch (error) { return error; }
  throw new Error('expected setup rejection');
};
const setupPayload = (setup: LocalSignerSetup, venue = 'okx'): unknown => {
  let received: unknown;
  class Engine { constructor(_venue: string, _options: string, json?: string) { received = JSON.parse(json!); } }
  setup.construct(Engine as never, venue, '{}');
  return received;
};

describe('shared required passphrase field policy', () => {
  it.each(passphraseFamilies)('%s declares both slot rules; only migrated OKX, Bitget and WEEX adopt dispatch signing', (venue) => {
    expect(deriveSignerSetupPolicy(venue)).toEqual({
      primary: { passphrase: 'requiredForPrivate' },
      secondary: { passphrase: 'requiredWithPrimaryFallback' },
    });
    const stripped = venue.startsWith('okx') || venue.startsWith('bitget');
    const migrated = stripped || venue === 'weex';
    expect(deriveBridgeConstruction(venue, {} as ExchangeOptions).dispatchSigning).toBe(migrated);
    const construct = () => new RustEngineExchange(venue, { key: 'key', secret: 'secret', passphrase: 'pass', localSigner: {} } as ExchangeOptions, new DefaultStore());
    if (stripped) {
      const exchange = construct();
      expect(exchange.options.key).toBeUndefined();
      expect(exchange.options.secret).toBeUndefined();
      expect(exchange.options.passphrase).toBeUndefined();
    } else if (migrated) {
      expect(construct).not.toThrow();
    } else {
      expect(construct).toThrow(/not supported/);
    }
  });

  it.each(passphraseFamilies)('%s rejects missing/empty primary passphrase only for complete private material', (venue) => {
    for (const passphrase of [undefined, '']) {
      expect(setupFailure(() => requiredSetup({ key: 'key', secret: 'secret', passphrase }, venue)))
        .toMatchObject({ kind: 'invalidInput', outcome: 'notSent', message: 'Rust engine primary signing credentials require a passphrase' });
      const setup = requiredSetup({ key: 'key', secret: 'secret', passphrase, publicDataOnly: true }, venue);
      expect(setup.primaryComplete).toBe(true);
      expect(setupPayload(setup, venue)).toEqual({ primary: { key: 'key', secret: 'secret', ...(passphrase === undefined ? {} : { passphrase }) } });
    }
    for (const input of [{}, { key: 'key' }, { secret: 'secret' }, { passphrase: '' }]) {
      const setup = requiredSetup(input, venue);
      expect(setup.primaryComplete).toBe(false);
      setup.dispose();
    }
  });

  it.each(passphraseFamilies)('%s keeps absent secondary passphrase absent while validating its primary fallback', (venue) => {
    const input = { key: 'key', secret: 'secret', passphrase: ' primary pass ', secondary_key: 'read', secondary_secret: 'read-secret' };
    const before = JSON.stringify(input);
    const setup = requiredSetup(input, venue);
    expect(setup.hasSecondary).toBe(true);
    const payload = setupPayload(setup, venue) as { secondary: Record<string, unknown> };
    expect(payload).toEqual({
      primary: { key: 'key', secret: 'secret', passphrase: ' primary pass ' },
      secondary: { key: 'read', secret: 'read-secret' },
    });
    expect(payload.secondary).not.toHaveProperty('passphrase');
    expect(JSON.stringify(input)).toBe(before);
  });

  it.each([false, true])('secondary checks remain active with publicDataOnly=%s and never fall back from explicit empty', (publicDataOnly) => {
    const complete = { key: 'key', secret: 'secret', passphrase: 'primary-pass', secondary_key: 'read', secondary_secret: 'read-secret', publicDataOnly };
    expect(setupFailure(() => requiredSetup({ ...complete, secondary_passphrase: '' })))
      .toMatchObject({ kind: 'invalidInput', outcome: 'notSent', message: 'Rust engine secondary signing credentials require a passphrase' });
    for (const passphrase of [undefined, '']) {
      // Partial primary material cannot fail a primary-private requirement first.
      expect(setupFailure(() => requiredSetup({ key: 'partial', passphrase, secondary_key: 'read', secondary_secret: 'read-secret', publicDataOnly })))
        .toMatchObject({ kind: 'invalidInput', outcome: 'notSent', message: 'Rust engine secondary signing credentials require a passphrase' });
    }
  });

  it('preserves a primary passphrase-only source without inventing primary key/signing material', () => {
    const input = { passphrase: 'fallback-only', secondary_key: 'read', secondary_secret: 'read-secret' };
    const setup = requiredSetup(input);
    expect(setup.primaryComplete).toBe(false);
    expect(setup.credentialPresence).toEqual({ key: false, secret: false });
    expect(setupPayload(setup)).toEqual({
      primary: { passphrase: 'fallback-only' },
      secondary: { key: 'read', secret: 'read-secret' },
    });
    expect(input).not.toHaveProperty('key');
    expect(input).not.toHaveProperty('secret');
  });

  it('prefers an own secondary passphrase and preserves whitespace, secret bytes and metadata without mutation', () => {
    const passphrase = '\u0085 \t\ufeff';
    const input = Object.freeze({
      key: ' primary ', secret: ' primary-secret ', passphrase,
      secondary_key: ' read ', secondary_secret: ' read-secret ', secondary_passphrase: ' own pass ',
      localSigner: Object.freeze({
        primary: Object.freeze({ stableId: 'account/7', trimCredential: true }),
        secondary: Object.freeze({ stableId: 'read/7', trimCredential: false }),
      }),
    });
    const before = JSON.stringify(input);
    const setup = requiredSetup(input);
    expect(setup.primaryComplete).toBe(true);
    expect(setupPayload(setup)).toEqual({
      primary: { key: ' primary ', secret: ' primary-secret ', passphrase, stableId: 'account/7', trimCredential: true },
      secondary: { key: ' read ', secret: ' read-secret ', passphrase: ' own pass ', stableId: 'read/7', trimCredential: false },
    });
    expect(JSON.stringify(input)).toBe(before);
    const readOnly = requiredSetup({ secondary_key: 'read', secondary_secret: 'read-secret', secondary_passphrase: 'own' });
    expect(setupPayload(readOnly)).toEqual({ secondary: { key: 'read', secret: 'read-secret', passphrase: 'own' } });
  });

  it('rejects malformed required passphrase values without exposing input or constructing an engine', () => {
    for (const passphrase of [null, 17, {}, ['secret-sentinel']]) {
      const failure = setupFailure(() => requiredSetup({ key: 'key', secret: 'secret', passphrase } as unknown as ExchangeOptions));
      expect(failure).toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
      expect(String(failure)).not.toContain('secret-sentinel');
      if (passphrase === null) continue;
      const secondaryFailure = setupFailure(() => requiredSetup({
        passphrase: 'valid-primary-fallback', secondary_key: 'read', secondary_secret: 'read-secret', secondary_passphrase: passphrase, publicDataOnly: true,
      } as unknown as ExchangeOptions));
      expect(secondaryFailure).toMatchObject({ kind: 'invalidInput', outcome: 'notSent', message: 'Rust engine secondary signing credentials require a passphrase' });
    }
  });

  it('preserves default optional policy and effective credential completeness before applying required rules', () => {
    for (const passphrase of [undefined, '']) {
      const setup = new LocalSignerSetup({ key: 'key', secret: 'secret', passphrase } as ExchangeOptions);
      expect(setup.primaryComplete).toBe(true);
      setup.dispose();
    }
    const explicitOptional = new LocalSignerSetup({
      key: 'key', secret: 'secret', passphrase: '',
      secondary_key: 'read', secondary_secret: 'read-secret', secondary_passphrase: '',
    } as ExchangeOptions, { primary: { passphrase: 'optional' }, secondary: { passphrase: 'optional' } });
    expect(setupPayload(explicitOptional)).toEqual({
      primary: { key: 'key', secret: 'secret', passphrase: '' },
      secondary: { key: 'read', secret: 'read-secret', passphrase: '' },
    });
    const input = { key: '\u0085 ', secret: 'secret', localSigner: { primary: { trimCredential: true } } };
    const setup = requiredSetup(input);
    expect(setup.primaryComplete).toBe(false);
    setup.dispose();
    const absent = requiredSetup({});
    expect(setupPayload(absent)).toEqual({});
    const publicAbsent = requiredSetup({ publicDataOnly: true });
    expect(setupPayload(publicAbsent)).toEqual({});
    const mexc = new LocalSignerSetup({ key: ' key ', secret: ' secret ' } as ExchangeOptions, deriveSignerSetupPolicy('mexc'));
    expect(setupPayload(mexc, 'mexc')).toEqual({ primary: { key: ' key ', secret: ' secret ', trimCredential: true } });
  });
});


it('keeps a secondary requiredForPrivate field required in public mode, matching native slot policy', () => {
  expect(() => new LocalSignerSetup({
    publicDataOnly: true, secondary_key: 'read', secondary_secret: 'read-secret',
  } as ExchangeOptions, { secondary: { passphrase: 'requiredForPrivate' } })).toThrow('secondary signing credentials require a passphrase');
});


it('preserves JSON null-as-None secondary fallback while explicit empty never inherits', () => {
  const options = {
    passphrase: 'primary-pass', secondary_key: 'read', secondary_secret: 'read-secret', secondary_passphrase: null,
  } as unknown as ExchangeOptions;
  const setup = new LocalSignerSetup(options, deriveSignerSetupPolicy('okx'));
  expect(setupPayload(setup)).toEqual({
    primary: { passphrase: 'primary-pass' },
    secondary: { key: 'read', secret: 'read-secret', passphrase: null },
  });
  expect(() => new LocalSignerSetup({ ...options, secondary_passphrase: '' }, deriveSignerSetupPolicy('okx'))).toThrow('secondary signing credentials require a passphrase');
});

describe('ApeX signer admission (RUST_APEXOMNI_SIGNER_ADMISSION.md)', () => {
  const apex = {
    engine: 'rust',
    key: 'api-key',
    secret: 'audit-apex-material',
    passphrase: 'pass',
    omniKeySeed: `0x${'11'.repeat(65)}`,
  } as unknown as ExchangeOptions;

  it('carries the OmniKey seed only in provider setup, as the zklinkSeed extra', () => {
    const setup = new LocalSignerSetup(apex, deriveSignerSetupPolicy('apexomni'), false, apex.omniKeySeed);
    const safe = withoutSigningCredentials(apex);
    expect(JSON.stringify(safe)).not.toContain('11'.repeat(65));
    expect(JSON.stringify(safe)).not.toContain('audit-apex-material');
    const calls: unknown[][] = [];
    class Engine {
      constructor(...args: unknown[]) {
        calls.push(args);
      }
    }
    setup.construct(Engine as never, 'apexomni', JSON.stringify(safe));
    expect(JSON.parse(calls[0][2] as string)).toEqual({
      primary: {
        key: 'api-key',
        secret: 'audit-apex-material',
        passphrase: 'pass',
        extra: { zklinkSeed: { value: apex.omniKeySeed, encoding: 'hex' } },
      },
    });
    expect(deriveBridgeConstruction('apexomni', apex).dispatchSigning).toBe(true);
  });

  it('keeps keyless and remote-signer ApeX accounts on TS until P8', () => {
    const remoteSigner = { sign: async () => ({}) } as unknown as ExchangeOptions['remoteSigner'];
    expect(runsOnRustEngine('apexomni', { engine: 'rust', secret: '', remoteSigner })).toBe(false);
    expect(runsOnRustEngine('apexomni', { engine: 'rust', secret: 'local' })).toBe(true);
  });
});
