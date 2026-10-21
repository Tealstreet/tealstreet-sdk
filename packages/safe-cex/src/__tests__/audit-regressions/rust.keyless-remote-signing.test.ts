import type { ExchangeName, ExchangeOptions, RemoteSigner } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { exchangeRegistry } from '../../exchanges/registry';
import {
  isKeylessRemoteSigned,
  runsOnRustEngine,
  RUST_VENUES,
  RustEngineExchange,
} from '../../exchanges/rust/rust.exchange';
import { createExchange } from '../../index';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';

const remoteSigner: RemoteSigner = {
  sign: async () => ({ id: 'never-called' }),
  isAvailable: () => true,
  onAvailabilityChange: () => () => {},
};

// exchange-server's hosted shape: the browser holds the secret and signs every request.
const hosted = (extra: Partial<ExchangeOptions> = {}): ExchangeOptions => ({
  key: 'hosted-api-key',
  secret: '',
  remoteSigner,
  privateOnly: true,
  engine: 'rust',
  ...extra,
});

// Lighter runs read-only without a secret; Paradex and Polymarket sign with `key`.
const engineSigned = new Set<ExchangeName>(['lighter', 'lighter-robinhood', 'paradex', 'polymarket']);

describe('keyless remote-signed accounts never start on an engine that cannot sign', () => {
  afterEach(() => vi.restoreAllMocks());

  it('routes every Rust venue family to TS except those that sign with key or none', () => {
    for (const name of RUST_VENUES) {
      expect(runsOnRustEngine(name, hosted()), name).toBe(engineSigned.has(name));
    }
    // A local secret or an AsterDEX V3 wallet key signs on the engine as before.
    expect(runsOnRustEngine('bybit', hosted({ secret: 'local-secret' }))).toBe(true);
    expect(runsOnRustEngine('asterdex', hosted({ auth: { privateKey: '0xabc' } }))).toBe(true);
    expect(runsOnRustEngine('hyperliquid', { engine: 'rust', secret: '' })).toBe(true);
  });

  it('createExchange builds the TS venue for a hosted account in each opt-in family', async () => {
    vi.spyOn(virtualClock, 'start').mockResolvedValue(undefined as never);
    const families: ExchangeName[] = [
      'binance',
      'binancespot',
      'bybit',
      'okx',
      'bitget',
      'blofin',
      'apexomni',
      'asterdex',
      'backpack',
      'bingx',
      'bitrue',
      'breakoutprop',
      'cryptocom',
      'grvt',
      'hyperliquid',
      'hyperliquidspot',
      'kraken',
      'krakenspot',
      'kucoin',
      'kucoinspot',
      'mexc',
      'phemex',
      'weex',
      'woo',
    ];
    for (const name of families) {
      // Venue construction checks the hosted fields the account form already requires.
      const exchange = await createExchange(name, hosted({ passphrase: 'hosted-passphrase' }));
      try {
        expect(exchange, name).not.toBeInstanceOf(RustEngineExchange);
      } finally {
        exchange.dispose();
      }
    }
  });

  it('a directly built engine refuses a hosted account at startup with the reason', async () => {
    for (const name of ['hyperliquid', 'bybit', 'tealstreet-dex', 'coinbase'] as ExchangeName[]) {
      const exchange = new RustEngineExchange(name, hosted(), new DefaultStore());
      await expect(exchange.start(), name).rejects.toThrow('keyless remote-signed accounts run on the TS venue');
      exchange.dispose();
    }
  });

  it('default-Rust names (Orderly brokers, Coinbase, TxFlow) still have no TS venue to route to', async () => {
    // Names whose registry entry is the engine itself. Restoring a TS loader for them
    // (RUST_REMOTE_SIGNING.md) flips this test.
    const engineOnly: ExchangeName[] = [];
    for (const name of RUST_VENUES) if ((await exchangeRegistry[name]()) === RustEngineExchange) engineOnly.push(name);
    expect(engineOnly.sort()).toEqual(
      ['coinbase', 'coinbasespot', 'kodiakfi', 'mode', 'salsadex', 'tealstreet-dex', 'txflow', 'what', 'woofi'].sort(),
    );
    for (const name of engineOnly) expect(isKeylessRemoteSigned(name, hosted()), name).toBe(true);
  });
});
