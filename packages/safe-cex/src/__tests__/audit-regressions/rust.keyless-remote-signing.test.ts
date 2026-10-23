import type { ExchangeName, ExchangeOptions, RemoteSigner } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { exchangeRegistry } from '../../exchanges/registry';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import {
  runsOnRustEngine,
  RUST_VENUES,
  RustEngineExchange,
} from '../../exchanges/rust/rust.exchange';
import { hostedSigningVenues, isKeylessRemoteSigned } from '../../exchanges/hostedSigningVenues';
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

  it('routes hosted accounts to Rust only with native signing or a declared provider', () => {
    for (const name of RUST_VENUES) {
      expect(runsOnRustEngine(name, hosted()), name).toBe(engineSigned.has(name) || deriveRemoteSigningPolicy(name)?.enabled === true);
    }
    // A local secret or an AsterDEX V3 wallet key signs on the engine as before.
    expect(runsOnRustEngine('bybit', hosted({ secret: 'local-secret' }))).toBe(true);
    expect(runsOnRustEngine('asterdex', hosted({ auth: { privateKey: '0xabc' } }))).toBe(true);
    expect(runsOnRustEngine('hyperliquid', { engine: 'rust', secret: '' })).toBe(true);
    // The wallet-key exemption is AsterDEX's own field, not a pass for other venues.
    expect(runsOnRustEngine('blofin', hosted({ auth: { privateKey: '0xabc' } }))).toBe(false);
    // Hosted Paradex sends its signing key in `key` with both addresses, as exchange-server does.
    const paradex = hosted({ key: '0x' + '1'.repeat(64), paradexAddress: '0xabc', subkeyAddress: '0xdef' });
    expect(runsOnRustEngine('paradex', paradex)).toBe(true);
    expect(new RustEngineExchange('paradex', paradex, new DefaultStore()).isPublicDataOnly).toBe(false);
    // Spot or not, a hosted Hyperliquid account signs through the browser on TS.
    expect(runsOnRustEngine('hyperliquid', hosted({ enableSpotMarkets: true }))).toBe(false);
  });

  it('createExchange admits declared providers and retains TS fallback for unadopted hosted families', async () => {
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
        expect(exchange instanceof RustEngineExchange, name).toBe(deriveRemoteSigningPolicy(name)?.enabled === true);
      } finally {
        exchange.dispose();
      }
    }
    // Loads and constructs two dozen venue modules.
  }, 60_000);

  it('a directly built engine refuses a hosted account at startup with the reason', async () => {
    for (const name of ['hyperliquid', 'blofin', 'tealstreet-dex', 'coinbase'] as ExchangeName[]) {
      const exchange = new RustEngineExchange(name, hosted(), new DefaultStore());
      await expect(exchange.start(), name).rejects.toThrow('keyless remote-signed accounts run on the TS venue');
      exchange.dispose();
    }
  });

  it('every default-Rust name has a kept TS venue for hosted accounts except TxFlow', async () => {
    // Names whose registry entry is the engine itself. TxFlow had no hosted users on master.
    const engineOnly: ExchangeName[] = [];
    for (const name of RUST_VENUES) if ((await exchangeRegistry[name]()) === RustEngineExchange) engineOnly.push(name);
    expect(engineOnly.sort()).toEqual(
      ['coinbase', 'coinbasespot', 'kodiakfi', 'mode', 'salsadex', 'tealstreet-dex', 'txflow', 'what', 'woofi'].sort(),
    );
    for (const name of engineOnly) expect(isKeylessRemoteSigned(name, hosted()), name).toBe(true);
    expect(engineOnly.filter((name) => !hostedSigningVenues[name])).toEqual(['txflow']);
  }, 60_000);
});
