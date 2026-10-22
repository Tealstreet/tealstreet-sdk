import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';

import { describe, expect, it } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { LocalSignerSetup, withoutSigningCredentials } from '../../exchanges/rust/localSignerSetup';

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
      primary: { key: input.key, secret: input.secret },
      secondary: { key: input.secondary_key, secret: input.secondary_secret },
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
  it('rejects incomplete secondary setup before a generic exchange can retain it', () => {
    for (const read of [
      { secondary_key: 'primary-key' },
      { secondary_secret: 'audit-secret-sentinel' },
      { secondary_passphrase: '' },
    ])
      expect(
        () => new LocalSignerSetup({ ...withoutSigningCredentials(options()), ...read } as ExchangeOptions),
      ).toThrow(/nonempty key and secret/);
  });
});


it.each(['binance', 'binancepm', 'binanceinverse', 'binancespot'] as const)(
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
