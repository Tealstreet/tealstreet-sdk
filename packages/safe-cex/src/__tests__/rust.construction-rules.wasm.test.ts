import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../types';

import { afterEach, expect, it } from 'vitest';

import { ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE } from '../exchanges/asterdex/asterdex.types';
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
