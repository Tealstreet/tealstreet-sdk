import { describe, expect, it } from 'vitest';
import golden from '../../exchanges/rust/asterdex-signer-setup.golden.json';
import { mapAsterDEXSignerSetup } from '../../exchanges/rust/asterdexSignerSetup';
import { signHostedHttp } from '../../exchanges/rust/legacySigning';
import { LocalSignerSetup, withoutSigningCredentials } from '../../exchanges/rust/localSignerSetup';
import { deriveBridgeConstruction } from '../../exchanges/rust/construction';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import type { ExchangeOptions } from '../../types';

// Native setup runs the same golden in sdk-rs/registry/tests/asterdex_setup.rs.
const expand = (value: unknown): unknown => {
  if (typeof value === 'string') return value.replace(/<hex32:([0-9a-f]{2})>/g, (_, byte: string) => byte.repeat(32));
  if (Array.isArray(value)) return value.map(expand);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, expand(v)]));
  return value;
};

const accountOptions = (input: Record<string, unknown>): ExchangeOptions => {
  const { apiVersion, ...rest } = input;
  return { ...rest, ...(apiVersion ? { extra: { asterdexApiVersion: apiVersion } } : {}) } as unknown as ExchangeOptions;
};

const setupJson = (options: ExchangeOptions): unknown => {
  const mapped = mapAsterDEXSignerSetup(options);
  const setup = new LocalSignerSetup(mapped.setupOptions, {}, mapped.secondarySigningFromPrimary);
  let captured: string | undefined;
  class Capture {
    constructor(_venue: string, _options: string, json?: string) {
      captured = json;
    }
  }
  setup.construct(Capture as never, 'asterdex', '{}');
  return { setup: JSON.parse(captured ?? 'null'), publicSetup: mapped.publicSetup };
};

describe('AsterDEX trusted setup mapping', () => {
  it('admits AsterDEX local signer setup instead of refusing it', () => {
    expect(deriveBridgeConstruction('asterdex', {} as ExchangeOptions).dispatchSigning).toBe(true);
    const exchange = new RustEngineExchange(
      'asterdex',
      { key: 'api-key', secret: 'api-secret', localSigner: { primary: { stableId: 'account/aster-1' } } } as ExchangeOptions,
      new DefaultStore(),
    );
    expect(exchange.options).not.toHaveProperty('secret');
    exchange.dispose();
  });

  for (const raw of golden.cases) {
    const testCase = expand(raw) as typeof raw;
    it(`matches the shared golden: ${testCase.name}`, () => {
      const { setup, publicSetup } = setupJson(accountOptions(testCase.input)) as {
        setup: unknown;
        publicSetup: Record<string, unknown>;
      };
      const expected = testCase.expected as Record<string, unknown>;
      expect(setup).toEqual(expected.setup);
      expect(publicSetup).toEqual({
        apiVersion: expected.apiVersion,
        ...(expected.signerAddress ? { signerAddress: expected.signerAddress } : {}),
        ...(expected.readSignerAddress ? { readSignerAddress: expected.readSignerAddress } : {}),
      });
    });
  }

  it('refuses API wallet keys under V1 without a main wallet before setup', () => {
    expect(() =>
      mapAsterDEXSignerSetup({ key: '0x1111111111111111111111111111111111111111', secret: '12'.repeat(32) } as ExchangeOptions),
    ).toThrow(/^Aster API Wallet\/vault keys require the Main Wallet Address\./);
  });

  it('strips the auth wallet key from generic options but keeps public addresses', () => {
    const options = {
      key: 'k',
      secret: 's',
      auth: { userAddress: '0xuser', signerAddress: '0xsigner', privateKey: 'wallet-material' },
    } as ExchangeOptions;
    expect(withoutSigningCredentials(options)).toEqual({ auth: { userAddress: '0xuser', signerAddress: '0xsigner' } });
    expect(options.auth?.privateKey).toBe('wallet-material');
  });

  it('refuses hosted HTTP signing for AsterDEX instead of using another family codec', async () => {
    const host = { hostedSigning: () => Promise.reject(new Error('provider must not be asked')) };
    const response = await signHostedHttp('asterdex', {} as ExchangeOptions, true, host as never, {
      id: 'request-1',
      config: { method: 'get', url: 'https://fapi.asterdex.com/fapi/v1/account' },
    } as never);
    expect(response).toEqual({ id: 'request-1', error: 'Hosted HTTP signing is not supported for this venue yet' });
  });
});
