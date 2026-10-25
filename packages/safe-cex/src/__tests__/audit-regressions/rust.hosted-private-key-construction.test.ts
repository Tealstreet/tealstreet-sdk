import type { ExchangeName, ExchangeOptions, RemoteSigner } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';
import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const privateKey = `0x${'1'.padStart(64, '0')}`;
const remote = (): RemoteSigner => ({
  sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {},
});
const families = ['paradex', 'polymarket'] as const;
const accounts: RustEngineExchange[] = [];
const construct = (name: ExchangeName, options: Partial<ExchangeOptions>) => {
  const exchange = new RustEngineExchange(name, {
    key: privateKey, secret: '', paradexAddress: '0x123', subkeyAddress: '0x456', ...options,
  }, new DefaultStore());
  accounts.push(exchange);
  return exchange;
};
afterEach(() => accounts.splice(0).forEach((exchange) => exchange.dispose()));

describe('hosted Rust construction refuses wallet material before retaining options', () => {
  for (const name of families) {
    for (const mode of ['hostedMode', 'remoteSigner'] as const) {
      it.each([privateKey, privateKey.slice(2)])(`${name} ${mode} refuses a parsed private key: %s`, (key) => {
        const provider = remote();
        const load = vi.fn();
        setWasmEngineLoader(load);
        let error: unknown;
        try { construct(name, { key, [mode]: mode === 'hostedMode' ? true : provider }); }
        catch (caught) { error = caught; }
        expect(error).toBeInstanceOf(ExchangeError);
        expect(error).toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
        expect(String(error)).not.toContain(key);
        expect(load).not.toHaveBeenCalled();
        expect(provider.sign).not.toHaveBeenCalled();
      });

      it(`${name} ${mode} also refuses private material on a public-only account`, () => {
        expect(() => construct(name, {
          publicDataOnly: true, [mode]: mode === 'hostedMode' ? true : remote(),
        })).toThrowError(ExchangeError);
      });

      it.each(['secret', 'starkPrivateKey'] as const)(`${name} ${mode} refuses %s`, (field) => {
        expect(() => construct(name, {
          key: '', [field]: 'private-material-sentinel',
          [mode]: mode === 'hostedMode' ? true : remote(),
        })).toThrowError(ExchangeError);
      });
      it(`${name} ${mode} permits absent material and public account addresses`, () => {
        expect(() => construct(name, {
          key: '', secret: '', starkPrivateKey: '',
          [mode]: mode === 'hostedMode' ? true : remote(),
        })).not.toThrow();
      });
    }

    it(`${name} keeps the local private-key mode`, () => {
      expect(() => construct(name, { hostedMode: false })).not.toThrow();
    });
    it(`${name} does not infer hosted mode from a payload callback`, () => {
      expect(() => construct(name, { payloadSigner: vi.fn() })).not.toThrow();
    });
  }

  for (const mode of ['hostedMode', 'remoteSigner'] as const) {
    const intent = () => ({ [mode]: mode === 'hostedMode' ? true : remote() });
    it(`Paradex ${mode} admits the same felt as public only under the payloadSigner contract`, () => {
      expect(() => construct('paradex', { ...intent(), payloadSigner: vi.fn() })).not.toThrow();
    });
    it(`Paradex ${mode} does not let payloadSigner admit secret material`, () => {
      expect(() => construct('paradex', {
        ...intent(), payloadSigner: vi.fn(), secret: 'private-material-sentinel',
      })).toThrowError(ExchangeError);
    });
    it.each(['apiSecret', 'apiPassphrase'] as const)(`Polymarket ${mode} refuses L2 %s`, (field) => {
      expect(() => construct('polymarket', {
        ...intent(), key: '', polymarket: { [field]: 'private-material-sentinel' },
      })).toThrowError(ExchangeError);
    });
    it(`Polymarket ${mode} retains public L2 metadata`, () => {
      expect(() => construct('polymarket', {
        ...intent(), key: '', polymarket: {
          apiKey: 'public-api-id', funderAddress: '0x123', signerType: 1,
          marketSlugs: ['public-market'], apiSecret: '', apiPassphrase: '',
        },
      })).not.toThrow();
    });
  }
});

describe('hosted setup refusal metadata and callback custody', () => {
  const material = 'private-material-sentinel';
  const cases: Array<[ExchangeName, Partial<ExchangeOptions>]> = [
    ['paradex', { key: ' ' }],
    ['paradex', { key: privateKey, payloadSigner: vi.fn(), secret: material }],
    ['paradex', { key: '', payloadSigner: vi.fn(), starkPrivateKey: material }],
    ['polymarket', { key: '', polymarket: { apiSecret: material } }],
    ['polymarket', { key: '', polymarket: { apiPassphrase: material } }],
  ];
  for (const mode of ['hostedMode', 'remoteSigner'] as const) {
    it.each(cases)(`${mode} %s material refuses with typed metadata before any provider`, (name, fields) => {
      const provider = remote();
      const load = vi.fn();
      setWasmEngineLoader(load);
      let error: unknown;
      try { construct(name, { ...fields, [mode]: mode === 'hostedMode' ? true : provider }); }
      catch (caught) { error = caught; }
      expect(error).toBeInstanceOf(ExchangeError);
      expect(error).toMatchObject({
        kind: 'invalidInput', outcome: 'notSent',
        message: 'Hosted Rust accounts accept public signing metadata only',
      });
      expect(String(error)).not.toContain(material);
      expect(provider.sign).not.toHaveBeenCalled();
      expect(load).not.toHaveBeenCalled();
    });
    it(`${mode} a non-callable payloadSigner cannot classify key as public`, () => {
      expect(() => construct('paradex', {
        [mode]: mode === 'hostedMode' ? true : remote(),
        payloadSigner: true as unknown as ExchangeOptions['payloadSigner'],
      })).toThrowError(ExchangeError);
    });
  }
});

describe('hosted top-level passphrase custody', () => {
  it.each(families)('%s refuses a passphrase before options retention', (name) => {
    let error: unknown;
    try { construct(name, { key: '', hostedMode: true, passphrase: 'private-material-sentinel' }); }
    catch (caught) { error = caught; }
    expect(error).toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
  });
});
