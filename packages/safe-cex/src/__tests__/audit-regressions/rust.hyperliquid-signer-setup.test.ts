import { describe, expect, it } from 'vitest';
import vectors from '../../exchanges/rust/hyperliquid-signer-setup.golden.json';
import { mapHyperliquidSignerSetup } from '../../exchanges/rust/hyperliquidSignerSetup';
import { deriveSignerSetupPolicy } from '../../exchanges/rust/construction';
import { LocalSignerSetup } from '../../exchanges/rust/localSignerSetup';
import type { ExchangeOptions } from '../../types';
import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { DefaultStore } from '../../store/store.base';

describe('Hyperliquid trusted ScalarOnly setup capsule', () => {
  for (const venue of ['hyperliquid', 'hyperliquidspot'] as const) {
    for (const row of vectors) {
      it(`${venue}: ${row.name} reaches the one native decoder unchanged`, () => {
        const options = {
          key: '0x1111111111111111111111111111111111111111',
          applicationId: '0x2222222222222222222222222222222222222222',
          vault: '0x3333333333333333333333333333333333333333',
          secret: row.scalar,
        } as ExchangeOptions;
        const mapped = mapHyperliquidSignerSetup(options);
        const setup = new LocalSignerSetup(mapped, deriveSignerSetupPolicy(venue));
        let captured: string | undefined;
        class Capture {
          constructor(_venue: string, _options: string, json?: string) { captured = json; }
        }
        setup.construct(Capture as never, venue, '{}');
        expect(JSON.parse(captured!)).toEqual({ primary: {
          secret: row.scalar, secretEncoding: 'hex',
        } });
        expect(options.key).toBe('0x1111111111111111111111111111111111111111');
        expect(mapped.applicationId).toBe(options.applicationId);
        expect(mapped.vault).toBe(options.vault);
        expect(() => setup.construct(Capture as never, venue, '{}')).toThrow(/already been consumed/);
      });
    }
  }

  it('refuses secondary setup instead of fabricating an agent credential', () => {
    for (const extra of [
      { secondary_key: '' }, { secondary_secret: 'secret' }, { secondary_passphrase: '' },
      { localSigner: { secondary: { stableId: 'secondary' } } },
    ]) {
      expect(() => mapHyperliquidSignerSetup({ ...extra, key: 'account', secret: 'scalar' } as ExchangeOptions))
        .toThrow('Invalid Hyperliquid local signer setup');
    }
  });

  it('refuses passphrase metadata in both localSigner slots', () => {
    for (const slot of ['primary', 'secondary']) {
      expect(() => new LocalSignerSetup(mapHyperliquidSignerSetup({
        secret: vectors[0].scalar, localSigner: { [slot]: { passphrase: 'ignored' } },
      } as unknown as ExchangeOptions), deriveSignerSetupPolicy('hyperliquid')))
        .toThrow(slot === 'primary' ? 'Invalid local signer setup metadata' : 'Invalid Hyperliquid local signer setup');
    }
  });

  for (const venue of ['hyperliquid', 'hyperliquidspot'] as const) {
    it(`${venue}: the real constructor discards legacy passphrases without changing setup or readiness`, async () => {
      let expected: unknown[] | undefined;
      for (const passphrase of [undefined, '', 'ignored-passphrase-sentinel']) {
        const scalar = vectors[0].scalar;
        const options = {
          key: '0x1111111111111111111111111111111111111111',
          applicationId: '0x2222222222222222222222222222222222222222',
          vault: '0x3333333333333333333333333333333333333333',
          secret: scalar,
          passphrase,
          localSigner: { primary: { secretEncoding: 'utf8', stableId: 'caller/agent' } },
        } as ExchangeOptions;
        const setup = new LocalSignerSetup(mapHyperliquidSignerSetup(options), deriveSignerSetupPolicy(venue));
        expect(setup.credentialPresence).toEqual({ key: false, secret: true });
        setup.dispose();
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
        const exchange = new RustEngineExchange(venue, options, new DefaultStore());
        try {
          expect(exchange.isPublicDataOnly).toBe(false);
          expect(Reflect.get(exchange, 'canSign')).toBe(true);
          expect(exchange.options.secret).toBeUndefined();
          expect(exchange.options).not.toHaveProperty('passphrase');
          expect(JSON.stringify(exchange.options)).not.toContain('ignored-passphrase-sentinel');
          await expect(exchange.start()).rejects.toBe(stop);
          expect(received?.[0]).toBe(venue);
          expect(received).toEqual(expected ??= received);
          const publicOptions = JSON.parse(received?.[1] as string);
          expect(publicOptions).toMatchObject({ key: options.key, applicationId: options.applicationId, vault: options.vault });
          expect(publicOptions).not.toHaveProperty('secret');
          expect(publicOptions).not.toHaveProperty('passphrase');
          expect(JSON.stringify(received)).not.toContain('ignored-passphrase-sentinel');
          expect(JSON.stringify(publicOptions)).not.toContain(scalar);
          expect(JSON.parse(received?.[2] as string)).toEqual({ primary: {
            secret: scalar, secretEncoding: 'hex', stableId: 'caller/agent',
          } });
        } finally {
          exchange.dispose();
          setWasmEngineLoader(loadNodeEngine);
          setRustEngineTransport(nativeTransport);
        }
      }
    });
  }
});
