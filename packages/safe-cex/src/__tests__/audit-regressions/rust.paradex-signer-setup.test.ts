import { describe, expect, it } from 'vitest';
import { mapParadexSignerSetup } from '../../exchanges/rust/paradexSignerSetup';
import { deriveSignerSetupPolicy, derivePublicMode } from '../../exchanges/rust/construction';
import { LocalSignerSetup } from '../../exchanges/rust/localSignerSetup';
import type { ExchangeOptions } from '../../types';
import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { DefaultStore } from '../../store/store.base';

const STARK = `0x${'0'.repeat(63)}1`;
const ETHEREUM = '11'.repeat(32);
const ACCOUNT = { paradexAddress: '0x0123', subkeyAddress: '0x456' };

describe('Paradex trusted Stark setup capsule', () => {
  for (const key of [STARK, ETHEREUM]) {
    it(`carries only the key text to the private capsule (${key.length} characters)`, () => {
      const options = { ...ACCOUNT, key } as ExchangeOptions;
      const setup = new LocalSignerSetup(mapParadexSignerSetup(options), deriveSignerSetupPolicy('paradex'));
      expect(setup.credentialPresence).toEqual({ key: false, secret: true });
      let captured: string | undefined;
      class Capture {
        constructor(_venue: string, _options: string, json?: string) { captured = json; }
      }
      setup.construct(Capture as never, 'paradex', '{}');
      expect(JSON.parse(captured!)).toEqual({ primary: { secret: key } });
      expect(options.key).toBe(key);
      expect(() => setup.construct(Capture as never, 'paradex', '{}')).toThrow(/already been consumed/);
    });
  }

  it('refuses secondary and caller signer metadata', () => {
    for (const extra of [
      { secondary_key: '' }, { secondary_secret: 'secret' }, { secondary_passphrase: '' },
      { localSigner: { primary: { stableId: 'caller' } } },
    ]) {
      expect(() => mapParadexSignerSetup({ ...extra, ...ACCOUNT, key: STARK } as ExchangeOptions))
        .toThrow('Invalid Paradex local signer setup');
    }
  });

  it('is private only with a key and both public addresses, and public without the key', () => {
    const mode = (options: ExchangeOptions) => {
      const setup = new LocalSignerSetup(mapParadexSignerSetup(options), deriveSignerSetupPolicy('paradex'));
      const result = derivePublicMode('paradex', options, undefined, setup.credentialPresence);
      setup.dispose();
      return result;
    };
    expect(mode({ ...ACCOUNT, key: STARK } as ExchangeOptions)).toEqual({ canSign: true, publicDataOnly: false });
    expect(mode({ ...ACCOUNT, key: '' } as ExchangeOptions)).toEqual({ canSign: false, publicDataOnly: true });
    expect(mode({ paradexAddress: '0x0123', key: STARK } as ExchangeOptions).publicDataOnly).toBe(true);
  });

  it('the real constructor keeps signing material out of engine options and drops legacy fields', async () => {
    let expected: unknown[] | undefined;
    for (const legacy of [{}, { secret: 'ignored-secret-sentinel', passphrase: 'ignored-passphrase-sentinel' }]) {
      const options = { ...ACCOUNT, key: STARK, ...legacy } as ExchangeOptions;
      let received: unknown[] | undefined;
      const stop = new Error('constructor boundary reached');
      class Capture {
        constructor(...args: unknown[]) { received = args; throw stop; }
      }
      setWasmEngineLoader(async () => ({ WasmEngine: Capture as never }));
      setRustEngineTransport(() => ({
        now: () => 1000,
        http: async () => { throw new Error('no HTTP at constructor boundary'); },
        connectSocket: () => { throw new Error('no socket at constructor boundary'); },
        setTimeout, clearTimeout,
      }) as never);
      const exchange = new RustEngineExchange('paradex', options, new DefaultStore());
      try {
        expect(exchange.isPublicDataOnly).toBe(false);
        expect(Reflect.get(exchange, 'canSign')).toBe(true);
        for (const field of ['key', 'secret', 'passphrase']) expect(exchange.options).not.toHaveProperty(field);
        await expect(exchange.start()).rejects.toBe(stop);
        expect(received).toEqual(expected ??= received);
        const engineOptions = JSON.parse(received?.[1] as string);
        expect(engineOptions).toMatchObject(ACCOUNT);
        for (const field of ['key', 'secret', 'passphrase', 'payloadSigner']) {
          expect(engineOptions[field] ?? undefined).toBeFalsy();
        }
        expect(received?.[1]).not.toContain(STARK.slice(2));
        expect(JSON.stringify(received)).not.toContain('sentinel');
        expect(JSON.parse(received?.[2] as string)).toEqual({ primary: { secret: STARK } });
      } finally {
        exchange.dispose();
        setWasmEngineLoader(loadNodeEngine);
        setRustEngineTransport(nativeTransport);
      }
    }
  });
});
