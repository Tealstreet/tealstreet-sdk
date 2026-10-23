import createHmac from 'create-hmac';
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

  it('surfaces a setup refusal at start, where the app routes it, with credentials already stripped', async () => {
    const exchange = new RustEngineExchange(
      'asterdex',
      { key: '0x1111111111111111111111111111111111111111', secret: '12'.repeat(32), extra: { asterdexApiVersion: 'v1' } } as ExchangeOptions,
      new DefaultStore(),
    );
    expect(exchange.options).not.toHaveProperty('secret');
    await expect(exchange.start()).rejects.toThrow(/^Aster API Wallet\/vault keys require the Main Wallet Address\./);
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

  it('signs hosted AsterDEX V1 HTTP with the owned primary over the TS venue query (R4)', async () => {
    const STAMP = 1_709_209_849_123;
    const calls: unknown[] = [];
    const host = {
      hostedSigningClock: () => ({ stampMs: STAMP, revision: '7' }),
      hostedSigning: async (input: { kind: string; payload?: string }) => {
        calls.push(input);
        if (input.kind === 'credential') return { credential: 'hosted-aster-key' };
        // Independent node HMAC; the Rust provider path has its own vector test.
        return {
          credential: 'hosted-aster-key',
          signature: createHmac('sha256', 'fixture-only-v1-secret').update(input.payload ?? '').digest('hex'),
        };
      },
    };
    const response = await signHostedHttp('asterdex', {} as ExchangeOptions, true, host as never, {
      id: 'request-1',
      config: { method: 'post', url: '/fapi/v1/order', data: JSON.stringify({ symbol: 'BTCUSDT', side: 'BUY', signature: 'stale' }) },
    } as never);
    const query = `symbol=BTCUSDT&side=BUY&recvWindow=20000&timestamp=${STAMP}`;
    expect(response.config?.params).toEqual({
      symbol: 'BTCUSDT',
      side: 'BUY',
      recvWindow: 20000,
      timestamp: STAMP,
      signature: createHmac('sha256', 'fixture-only-v1-secret').update(query).digest('hex'),
    });
    expect(response.config?.data).toBeUndefined();
    expect(response.config?.headers?.['X-MBX-APIKEY']).toBe('hosted-aster-key');
    expect(calls).toContainEqual(expect.objectContaining({ kind: 'http', payload: query }));
    // Public market data needs no signature; V3 wallet requests are never HMAC-signed.
    const unsigned = await signHostedHttp('asterdex', {} as ExchangeOptions, true, host as never, {
      id: 'request-2',
      config: { method: 'get', url: '/fapi/v1/klines', params: JSON.stringify({ symbol: 'BTCUSDT' }) },
    } as never);
    expect(unsigned.config?.params).toEqual({ symbol: 'BTCUSDT' });
    await expect(signHostedHttp('asterdex', {} as ExchangeOptions, true, host as never, {
      id: 'request-3',
      config: { method: 'get', url: '/fapi/v3/account' },
    } as never)).rejects.toThrow('not HMAC-signed');
  });
});
