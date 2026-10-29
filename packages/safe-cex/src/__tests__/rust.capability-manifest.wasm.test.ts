import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { BaseExchange, ExchangeCapabilities } from '../exchanges/base';
import type { ExchangeName, ExchangeOptions } from '../types';

import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { afterEach, expect, it, vi } from 'vitest';

import { exchangeRegistry } from '../exchanges/registry';
import { BETA_VENUES } from '../exchanges/rust/betaVenues';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';

// HyroTrader has no default endpoints: hosts configure them at build time, read when bybit.types loads.
vi.hoisted(() => {
  process.env.NEXT_PUBLIC_HYROTRADER_PROD_REST_URL ??= 'https://hyrotrader.capability-manifest.invalid';
  process.env.NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL ??= 'https://hyrotrader-simulator.capability-manifest.invalid';
});

// Regenerate with `corepack yarn workspace @tealstreet/safe-cex capability-manifest`, which rebuilds
// the wasm first; `test-wasm` fails on any difference, so the file is exactly what the runtime reports.
const MANIFEST_PATH = resolve(__dirname, '../exchanges/rust/capabilityManifest.json');
const WRITE_MANIFEST = process.env.SAFE_CEX_WRITE_CAPABILITY_MANIFEST === '1';
// Two engine constructions per beta venue; a cold runner can exceed the 5 s default.
const MANIFEST_TIMEOUT_MS = 120_000;

const ACCOUNT_OPTIONS: ExchangeOptions = {
  key: '0x1111111111111111111111111111111111111111',
  secret: `0x${'22'.repeat(32)}`,
  passphrase: 'capability-manifest-passphrase',
  applicationId: 'capability-manifest-application',
  apiKeyIndex: '2',
  starkPrivateKey: '0x1',
  subkeyAddress: '0x1111111111111111111111111111111111111111',
  grvtSignerAddress: '0x1111111111111111111111111111111111111111',
  grvtTradingAccountId: 'capability-manifest-account',
};

/** Venues whose secret has its own encoding; the values are well-formed and never sent. */
const CREDENTIAL_SHAPES: Partial<Record<ExchangeName, Partial<ExchangeOptions>>> = {
  kraken: { secret: Buffer.alloc(64, 0x22).toString('base64') },
  krakenspot: { secret: Buffer.alloc(64, 0x22).toString('base64') },
  lighter: { secret: `0x${'22'.repeat(40)}` },
};

const running: BaseExchange[] = [];
afterEach(() => {
  for (const exchange of running.splice(0)) exchange.dispose();
});

const noIo = () => {
  throw new Error('Capability loading must not issue transport work');
};
const transport: Transport = {
  now: () => 1_700_000_000_000,
  request: noIo,
  connect: noIo,
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
};

/** Construction through the runtime registry, as the app does; capabilities need no venue I/O. */
const loadCapabilities = async (name: ExchangeName, options: ExchangeOptions): Promise<ExchangeCapabilities> => {
  const Exchange = await exchangeRegistry[name]();
  const exchange = new Exchange(name, options, new DefaultStore());
  running.push(exchange);
  return exchange.loadCapabilities();
};

it(
  'the committed capability manifest is what every beta venue reports at runtime',
  async () => {
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => transport);
    const generated: Record<string, ExchangeCapabilities> = {};
    const credentialDependent: string[] = [];
    for (const name of [...BETA_VENUES].sort()) {
      const account = await loadCapabilities(name, { ...ACCOUNT_OPTIONS, ...CREDENTIAL_SHAPES[name] });
      const publicOnly = await loadCapabilities(name, { key: '', secret: '', publicDataOnly: true });
      const axes = new Set([...Object.keys(account), ...Object.keys(publicOnly)]) as Set<keyof ExchangeCapabilities>;
      for (const axis of axes) if (account[axis] !== publicOnly[axis]) credentialDependent.push(`${name}.${axis}`);
      generated[name] = account;
    }
    // An account-dependent axis would make the manifest describe one account, not the venue.
    expect(credentialDependent).toEqual([]);

    const serialized = `${JSON.stringify(generated, null, 2)}\n`;
    if (WRITE_MANIFEST) writeFileSync(MANIFEST_PATH, serialized);
    expect(
      readFileSync(MANIFEST_PATH, 'utf8'),
      'stale capabilityManifest.json: run the capability-manifest script',
    ).toBe(serialized);
  },
  MANIFEST_TIMEOUT_MS,
);
