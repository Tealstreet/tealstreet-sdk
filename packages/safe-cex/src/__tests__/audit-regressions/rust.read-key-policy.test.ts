// Declared read-key policy (docs/plans/RUST_READ_KEY_POLICY.md), JS setup side.
// No family declares a policy yet, so these drive it through the setup policy.
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../../types';
import type { SignerSetupPolicy } from '../../exchanges/rust/construction';

import { describe, expect, it } from 'vitest';

import { LocalSignerSetup, withoutSigningCredentials } from '../../exchanges/rust/localSignerSetup';
import { deriveBridgeConstruction, deriveSignerSetupPolicy } from '../../exchanges/rust/construction';
import rules from '../../exchanges/rust/construction-rules.json';

const SENTINEL = 'read-key-material-sentinel';
const base = { key: 'primary-key', secret: 'primary-material', publicDataOnly: false } as ExchangeOptions;
type Constructor = new (venue: string, optionsJson: string, signerSetupJson?: string) => WasmEngineLike;

const construct = (options: ExchangeOptions, policy: SignerSetupPolicy) => {
  const setup = new LocalSignerSetup(options, policy);
  const calls: string[][] = [];
  class Engine {
    constructor(...args: string[]) {
      calls.push(args);
    }
  }
  setup.construct(Engine as unknown as Constructor, 'probe', JSON.stringify(withoutSigningCredentials(options)));
  return { setup, optionsJson: calls[0][1], capsule: calls[0][2] };
};

describe('ValidateOnly read keys', () => {
  const validateOnly = { readCredentials: 'validateOnly' as const };

  it('carries only the validation result, never the read key', () => {
    for (const [read, valid] of [
      [{ secondary_key: 'read', secondary_secret: SENTINEL }, true],
      [{ secondary_key: 'read' }, false],
      [{ secondary_key: '', secondary_secret: SENTINEL }, false],
      [{ secondary_key: ' ', secondary_secret: SENTINEL }, true], // untrimmed, as in Rust
    ] as const) {
      for (const publicDataOnly of [false, true]) {
        const { setup, optionsJson, capsule } = construct({ ...base, ...read, publicDataOnly }, validateOnly);
        expect(setup.readCredentialsValid).toBe(valid);
        expect(setup.hasSecondary).toBe(false);
        expect(JSON.parse(optionsJson).readCredentialsValid).toBe(valid);
        expect(JSON.parse(capsule).secondary).toBeUndefined();
        expect(capsule).not.toContain(SENTINEL);
        expect(optionsJson).not.toContain(SENTINEL);
        expect(JSON.parse(capsule).primary).toEqual({ key: 'primary-key', secret: 'primary-material' });
      }
    }
  });

  it('sends nothing for an absent read key and never throws for an incomplete one', () => {
    const { setup, optionsJson } = construct(base, validateOnly);
    expect(setup.readCredentialsValid).toBeUndefined();
    expect(JSON.parse(optionsJson)).not.toHaveProperty('readCredentialsValid');
    expect(() => new LocalSignerSetup({ ...base, secondary_secret: SENTINEL }, validateOnly)).not.toThrow();
  });

  it('refuses read-identity setup metadata, which would describe an identity it never installs', () => {
    expect(() => new LocalSignerSetup({ ...base, localSigner: { secondary: { stableId: 'read' } } }, validateOnly))
      .toThrow('Invalid local signer setup metadata');
  });

  it('keeps large numeric options exact while adding the result', () => {
    const calls: string[][] = [];
    class Engine {
      constructor(...args: string[]) {
        calls.push(args);
      }
    }
    const setup = new LocalSignerSetup({ ...base, secondary_key: 'read', secondary_secret: SENTINEL }, validateOnly);
    // Hand-written JSON: a JS number literal would already have rounded.
    setup.construct(Engine as unknown as Constructor, 'probe', '{"lighterIntegratorAccountIndex":9007199254740993}');
    expect(calls[0][1]).toBe('{"readCredentialsValid":true,"lighterIntegratorAccountIndex":9007199254740993}');
    const empty = new LocalSignerSetup({ ...base, secondary_key: 'read' }, validateOnly);
    empty.construct(Engine as unknown as Constructor, 'probe', '{}');
    expect(calls[1][1]).toBe('{"readCredentialsValid":false}');
  });
});

describe('the unchanged default', () => {
  it('installs a complete read key and still rejects an incomplete one, exactly as before', () => {
    const { setup, optionsJson, capsule } = construct({ ...base, secondary_key: 'read', secondary_secret: SENTINEL }, {});
    expect(setup.readCredentialsValid).toBeUndefined();
    expect(setup.hasSecondary).toBe(true);
    expect(JSON.parse(capsule).secondary).toEqual({ key: 'read', secret: SENTINEL });
    expect(JSON.parse(optionsJson)).not.toHaveProperty('readCredentialsValid');
    expect(() => new LocalSignerSetup({ ...base, secondary_key: 'read' }, {}))
      .toThrow('Rust engine secondary signing keys require a nonempty key and secret');
  });

  it('declares no family yet, so every bridge record and setup policy is unchanged', () => {
    for (const name of Object.keys(rules.venues) as ExchangeName[]) {
      expect(deriveBridgeConstruction(name, base)).not.toHaveProperty('readCredentials');
      expect(deriveSignerSetupPolicy(name)).not.toHaveProperty('readCredentials');
    }
  });
});
