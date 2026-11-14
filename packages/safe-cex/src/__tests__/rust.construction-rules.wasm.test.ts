import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../types';

import { afterEach, expect, it, vi } from 'vitest';

import { ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE, resolveAsterDEXApiVersion } from '../exchanges/asterdex/asterdex.types';
import { deriveBridgeConstruction, deriveRemoteSigningPolicy } from '../exchanges/rust/construction';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import {
  RUST_VENUES,
  RustEngineExchange,
  setRustEngineTransport,
  setWasmEngineLoader,
} from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';

vi.mock('../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('./reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../exchanges/rust/betaVenues')>());
});

const WALLET = '0x1111111111111111111111111111111111111111';
const COINBASE_KEY = ['organizations', 'fixture', 'apiKeys', 'fixture'].join('/');
const PRIVATE_KEY = `0x${'22'.repeat(32)}`;
const running: RustEngineExchange[] = [];

it('compiled and synchronous construction policies agree before startup for every venue and Hyro mode', async () => {
  const { resolveConstruction } = await loadNodeEngine();
  expect(resolveConstruction).toBeTypeOf('function');
  for (const name of RUST_VENUES)
    for (const mode of [undefined, 'demo', 'production']) {
      const options = { key: '', secret: '', extra: { hyrotraderMode: mode } } as ExchangeOptions;
      const native = JSON.parse(resolveConstruction!(name, JSON.stringify({ hyrotrader: { mode } })));
      expect(native.bridge, `${name}/${mode}`).toEqual(deriveBridgeConstruction(name, options));
    }
});

afterEach(() => {
  for (const exchange of running.splice(0)) exchange.dispose();
});

/** The Rust registry owns these rules; the bridge only surfaces its message, before any I/O. */
const startRejects = async (name: ExchangeName, options: Partial<ExchangeOptions>, message: string | RegExp) => {
  setWasmEngineLoader(loadNodeEngine);
  const requests: string[] = [];
  const transport: Transport = {
    now: () => Date.now(),
    connect: (url) => {
      requests.push(url);
      return { send: () => {}, close: () => {} };
    },
    request: async ({ method, url }) => {
      requests.push(`${method} ${url}`);
      return { status: 500, headers: '', body: new Uint8Array() };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  setRustEngineTransport(() => transport);
  const exchange = new RustEngineExchange(
    name,
    { key: '', secret: '', ...options } as ExchangeOptions,
    new DefaultStore(),
  );
  running.push(exchange);
  await expect(exchange.start()).rejects.toThrow(message);
  expect(requests).toEqual([]);
};

it('AsterDEX wallet keys without the main wallet carry the message the app routes on', () =>
  startRejects(
    'asterdex',
    { key: WALLET, secret: PRIVATE_KEY, extra: { asterdexApiVersion: 'v1' } },
    ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE,
  ));

it('Polymarket private accounts need a wallet private key', () =>
  startRejects('polymarket', { key: 'not-a-key' }, 'Polymarket requires a local 32-byte wallet private key'));

it('Orderly private accounts need an application id', () =>
  startRejects('woofi', { key: '1'.repeat(32), secret: '1'.repeat(32) }, 'Orderly private applicationId is required'));

it('native private construction checks callback provider availability before unsupported custody and I/O', async () => {
  const { WasmEngine } = await loadNodeEngine();
  expect(
    () =>
      new WasmEngine(
        'txflow',
        JSON.stringify({
          key: 'key',
          remoteSigning: true,
          payloadSigner: true,
        }),
      ),
  ).toThrow('Delegated signing requires a configured callback provider');
});

it.each([
  'binance',
  'binancepm',
  'binanceinverse',
  'binancespot',
  'bybit',
  'bybitinverse',
  'bybitspot',
  'bybitdemo',
  'coinbase',
  'coinbasespot',
] as const)(
  'compiled %s admits owned PayloadSigner setup before startup without local credentials or I/O',
  async (name) => {
    setWasmEngineLoader(loadNodeEngine);
    let calls = 0;
    let ioCalls = 0;
    const io = () => {
      ioCalls++;
      throw new Error('Capability construction must not issue transport work');
    };
    setRustEngineTransport(() => ({
      now: () => 1700000000123,
      request: io,
      connect: io,
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
    }));
    const key = name === 'coinbase' || name === 'coinbasespot' ? COINBASE_KEY : 'owned-public';
    for (const secret of ['', 'unused-local']) {
      const exchange = new RustEngineExchange(
        name,
        {
          key,
          secret,
          payloadSigner: () => {
            calls++;
            return { signature: 'ab'.repeat(32) };
          },
        },
        new DefaultStore(),
      );
      running.push(exchange);
      await expect(exchange.loadCapabilities()).resolves.toBeDefined();
      expect(exchange.isPublicDataOnly).toBe(false);
      for (const field of ['key', 'secret', 'payloadSigner']) expect(exchange.options).not.toHaveProperty(field);
    }
    expect(calls).toBe(0);
    expect(ioCalls).toBe(0);
  },
);

it.each([
  [undefined, 'v3'],
  ['v1', 'v1'],
  ['v3', 'v3'],
] as const)('compiled Aster resolves API version with override %s', async (apiVersion, expected) => {
  const { WasmEngine } = await loadNodeEngine();
  const engine = new WasmEngine(
    'asterdex',
    JSON.stringify({
      key: WALLET,
      secret: PRIVATE_KEY,
      applicationId: WALLET,
      asterdex: { apiVersion },
    }),
  );
  try {
    expect(JSON.parse(engine.construction!())).toEqual({
      asterdexApiVersion: expected,
      bridge: {
        spot: false,
        demo: false,
        primaryCredentialsOnly: false,
        payloadSigning: false,
        apiKeyIndexRequired: false,
        isolatedBookSubscriptions: true,
        dispatchSigning: true,
      },
    });
    const actions = JSON.parse(engine.start(1_700_000_000_000));
    // The clock is a managed read: its URL rides in the Prepare packet.
    const url = (action: { url?: string; packet?: { url?: string } }) => action.url ?? action.packet?.url;
    expect(
      actions.some((action: { url?: string; packet?: { url?: string } }) =>
        url(action)?.includes(`/fapi/${expected}/time`),
      ),
    ).toBe(true);
  } finally {
    (engine as unknown as { free(): void }).free();
  }
});

it('compiled native and synchronous TS Aster version resolution agree on the same inputs', async () => {
  const { WasmEngine } = await loadNodeEngine();
  const base = { key: WALLET, secret: PRIVATE_KEY, applicationId: WALLET };
  const cases: ExchangeOptions[] = [
    base,
    { ...base, extra: { asterdexApiVersion: 'v1' } },
    { ...base, extra: { asterdexApiVersion: 'v3' } },
    { key: 'hmac', secret: 'hmac' },
    { key: '', secret: '', auth: { userAddress: WALLET, signerAddress: WALLET, privateKey: PRIVATE_KEY } },
    { ...base, auth: { userAddress: ' ', signerAddress: ' ', privateKey: ' ' } },
    { ...base, auth: { userAddress: ` ${WALLET} `, signerAddress: ` ${WALLET} `, privateKey: ` ${PRIVATE_KEY} ` } },
    { ...base, auth: { userAddress: 'invalid' } },
    { ...base, auth: { signerAddress: 'invalid' } },
    { ...base, auth: { privateKey: 'invalid' } },
    { ...base, applicationId: undefined },
  ];
  for (const options of cases) {
    const engine = new WasmEngine(
      'asterdex',
      JSON.stringify({
        ...options,
        publicDataOnly: true,
        asterdex: { ...options.auth, apiVersion: options.extra?.asterdexApiVersion },
      }),
    );
    try {
      expect(JSON.parse(engine.construction!()).asterdexApiVersion).toBe(resolveAsterDEXApiVersion(options));
    } finally {
      (engine as unknown as { free(): void }).free();
    }
  }
});

it.each([{ key: '' }, { paradexAddress: '' }, { subkeyAddress: '' }])(
  'compiled Paradex rejects raw private intent with missing fields before I/O: %j',
  (missing) =>
    startRejects(
      'paradex',
      {
        key: '0x1',
        paradexAddress: '0x123',
        subkeyAddress: '0x456',
        ...missing,
        publicDataOnly: false,
      },
      'Rust Paradex private accounts require a key, Paradex address and subkey address',
    ),
);

it.each(['hyperliquid', 'hyperliquidspot', 'txflow', 'polymarket', 'lighter', 'lighter-robinhood'] as const)(
  'compiled native payload refusal on %s keeps startup and validation classification before I/O',
  async (name) => {
    setWasmEngineLoader(loadNodeEngine);
    const requests: string[] = [];
    const transport: Transport = {
      now: () => Date.now(),
      connect: (url) => {
        requests.push(url);
        throw new Error('No socket expected');
      },
      request: async ({ url }) => {
        requests.push(url);
        throw new Error('No HTTP expected');
      },
      setTimeout: (fn, ms) => setTimeout(fn, ms),
      clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
    };
    setRustEngineTransport(() => transport);
    let signatures = 0;
    // A venue with hosted remote signing but no payload signing refuses in the bridge, at construction.
    const bridgeRefuses =
      deriveRemoteSigningPolicy(name)?.enabled === true && deriveRemoteSigningPolicy(name)?.payloadSupported === false;
    for (const secret of name.startsWith('lighter') || name === 'hyperliquidspot' ? ['local'] : ['', 'local']) {
      const options = {
        key: 'key',
        secret,
        apiKeyIndex: '2',
        testnet: true,
        payloadSigner: async () => {
          signatures++;
          return { signature: 'unexpected' };
        },
      };
      if (bridgeRefuses) {
        let failure: unknown;
        try {
          running.push(new RustEngineExchange(name, options, new DefaultStore()));
        } catch (error) {
          failure = error;
        }
        expect(failure).toMatchObject({
          message: 'This Rust account has no REST-only PayloadSigner lifecycle',
          kind: 'unsupported',
          outcome: 'notSent',
        });
        continue;
      }
      const exchange = new RustEngineExchange(name, options, new DefaultStore());
      running.push(exchange);
      // Retained fixtures bypass SDK admission; callback intent alone installs no native provider.
      const missingProvider = name === 'txflow';
      const kind = missingProvider ? 'invalidInput' : 'unsupported';
      const message = missingProvider
        ? 'Delegated signing requires a configured callback provider'
        : name === 'polymarket'
          ? 'Polymarket hosted/delegated signing is unsupported: the user WebSocket requires the API secret, which a signature-only provider cannot supply'
          : `Rust ${name} delegated signing is unsupported in this build. Remove payloadSigner and configure local signing credentials.`;
      await expect(exchange.start()).rejects.toMatchObject({ message, kind, outcome: 'notSent' });
      await expect(exchange.validateAccount()).resolves.toMatchObject({ kind, error: message });
    }
    expect(requests).toEqual([]);
    expect(signatures).toBe(0);
  },
);

it.each(['txflow'] as const)(
  'compiled native %s distinguishes missing providers from unsupported payload custody',
  async (name) => {
    const { WasmEngine } = await loadNodeEngine();
    for (const providerReady of [false, true]) {
      let failure: unknown;
      try {
        new WasmEngine(
          name,
          JSON.stringify({
            key: 'key',
            secret: 'local',
            payloadSigner: true,
            signing: providerReady ? {} : undefined,
          }),
        );
      } catch (error) {
        failure = error;
      }
      if (!(failure instanceof Error) || !('venueError' in failure) || typeof failure.venueError !== 'string') {
        throw new Error('Expected a structured native construction refusal');
      }
      expect(JSON.parse(failure.venueError), `${name} providerReady=${providerReady}`).toMatchObject({
        message: providerReady
          ? `Rust ${name} delegated signing is unsupported in this build. Remove payloadSigner and configure local signing credentials.`
          : 'Delegated signing requires a configured callback provider',
        kind: providerReady ? 'unsupported' : 'invalidInput',
        outcome: 'notSent',
      });
    }
  },
);

it('compiled construction retains the public payload bypass and refuses hosted Paradex without I/O', async () => {
  const { WasmEngine } = await loadNodeEngine();
  const engine = new WasmEngine('binance', JSON.stringify({ publicDataOnly: true, payloadSigner: true }));
  (engine as unknown as { free(): void }).free();
  let failure: unknown;
  try {
    new WasmEngine(
      'paradex',
      JSON.stringify({ key: 'public-subkey', paradexAddress: '0x1', subkeyAddress: '0x2', payloadSigner: true }),
    );
  } catch (error) {
    failure = error;
  }
  expect(failure).toBeInstanceOf(Error);
  expect(JSON.parse((failure as { venueError: string }).venueError)).toMatchObject({
    message: expect.stringContaining('Rust paradex delegated signing is unsupported in this build'),
    kind: 'unsupported',
    outcome: 'notSent',
  });
});

it('compiled constructor exposes structured native failure detail', async () => {
  const { WasmEngine } = await loadNodeEngine();
  let failure: unknown;
  try {
    new WasmEngine('binance', JSON.stringify({ payloadSigner: true }));
  } catch (error) {
    failure = error;
  }
  expect(failure).toBeInstanceOf(Error);
  expect(JSON.parse((failure as { venueError: string }).venueError)).toMatchObject({
    message: expect.stringContaining('payloadSigner requires a configured injected signing provider'),
    kind: 'unsupported',
    outcome: 'notSent',
  });
});
