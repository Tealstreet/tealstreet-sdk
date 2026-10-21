import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../types';

import { afterEach, expect, it } from 'vitest';

import { ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE, resolveAsterDEXApiVersion } from '../exchanges/asterdex/asterdex.types';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';

const WALLET = '0x1111111111111111111111111111111111111111';
const PRIVATE_KEY = `0x${'22'.repeat(32)}`;
const running: RustEngineExchange[] = [];

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
  const exchange = new RustEngineExchange(name, { key: '', secret: '', ...options } as ExchangeOptions, new DefaultStore());
  running.push(exchange);
  await expect(exchange.start()).rejects.toThrow(message);
  expect(requests).toEqual([]);
};

it('BreakoutProp market data still needs DXtrade credentials', () =>
  startRejects('breakoutprop', { publicDataOnly: true }, 'BreakoutProp market data requires a legacy DXtrade username and password'));

it('AsterDEX wallet keys without the main wallet carry the message the app routes on', () =>
  startRejects(
    'asterdex',
    { key: WALLET, secret: PRIVATE_KEY, extra: { asterdexApiVersion: 'v1' } },
    ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE,
  ));

it('Polymarket private accounts need a wallet private key', () =>
  startRejects('polymarket', { key: 'not-a-key' }, 'Polymarket requires a local 32-byte wallet private key'));

it('Orderly private accounts need an application id', () =>
  startRejects('tealstreet-dex', { key: '1'.repeat(32), secret: '1'.repeat(32) }, 'Orderly private applicationId is required'));

// The Rust local-signing rule (registry tests) backstops these; the bridge refuses
// keyless remote and payload signers first, so neither reaches the engine.
it('locally signing venues refuse a payload signer before any I/O', () =>
  startRejects(
    'binance',
    { key: 'key', payloadSigner: (async () => ({ signature: '' })) as unknown as ExchangeOptions['payloadSigner'] },
    'Rust binance does not support payloadSigner',
  ));

it.each([
  [undefined, 'v3'],
  ['v1', 'v1'],
  ['v3', 'v3'],
] as const)('compiled Aster resolves API version with override %s', async (apiVersion, expected) => {
  const { WasmEngine } = await loadNodeEngine();
  const engine = new WasmEngine('asterdex', JSON.stringify({
    key: WALLET, secret: PRIVATE_KEY, applicationId: WALLET,
    asterdex: { apiVersion },
  }));
  try {
    expect(JSON.parse(engine.construction!())).toEqual({ asterdexApiVersion: expected });
    const actions = JSON.parse(engine.start(1_700_000_000_000));
    expect(actions.some((action: { url?: string }) => action.url?.includes(`/fapi/${expected}/time`))).toBe(true);
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
    const engine = new WasmEngine('asterdex', JSON.stringify({
      ...options, publicDataOnly: true,
      asterdex: { ...options.auth, apiVersion: options.extra?.asterdexApiVersion },
    }));
    try {
      expect(JSON.parse(engine.construction!()).asterdexApiVersion).toBe(resolveAsterDEXApiVersion(options));
    } finally {
      (engine as unknown as { free(): void }).free();
    }
  }
});
