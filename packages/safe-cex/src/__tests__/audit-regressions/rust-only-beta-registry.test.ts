import type { ExchangeAccountConfig, ExchangeName } from '../../types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { createExchangeFromAccount } from '../../createExchangeFromAccount';
import { exchangeMetadata } from '../../exchangeMetadata';
import { exchangeRegistry } from '../../exchanges/registry';
import { exchangeRegistry as nativeRegistry } from '../../exchanges/registry.native';
import { assertBetaAccount, BETA_VENUES } from '../../exchanges/rust/betaVenues';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { createRustRegistry, getRegisteredExchangeNames } from '../../exchanges/rustRegistry';
import { createExchange, createPublicExchange, preloadExchange } from '../../index';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';

const state = vi.hoisted(() => {
  const calls: Array<{ name: string; options: unknown }> = [];
  class Rust {
    constructor(name: string, options: unknown) {
      calls.push({ name, options });
    }
  }
  return { calls, Rust };
});
vi.mock('../../exchanges/rust/rust.exchange', () => ({ RustEngineExchange: state.Rust }));
vi.mock('../../utils/virtualClock', () => ({ virtualClock: { start: vi.fn(), setServerTimeSource: vi.fn() } }));
const expected = [
  'binance',
  'binancepm',
  'binanceoptions',
  'binancespot',
  'binanceinverse',
  'bybit',
  'bybitspot',
  'bybitinverse',
  'bybitdemo',
  'hyrotrader',
  'okx',
  'okxdemo',
  'okxeu',
  'okxeudemo',
  'bitget',
  'bitgetusdc',
  'bitgetinverse',
  'kraken',
  'krakenspot',
  'hyperliquid',
  'hyperliquidspot',
  'lighter',
  'lighter-robinhood',
  'paradex',
  'bitunix',
  'phemex',
  'bingx',
  'apexomni',
  'blofin',
  'cryptocom',
  'asterdex',
  'mexc',
  'woo',
  'kucoin',
  'kucoinspot',
  'coinbase',
  'coinbasespot',
  'grvt',
  'kodiakfi',
  'mode',
  'what',
  'woofi',
  'backpack',
  'bitrue',
];
const out = Object.keys(exchangeMetadata).filter((name) => !expected.includes(name)) as ExchangeName[];
const unsupported = { kind: 'unsupported', outcome: 'notSent' };
beforeEach(() => {
  state.calls.length = 0;
  vi.mocked(virtualClock.start).mockClear();
});
describe('Rust-only beta construction', () => {
  it('admits the live Bitrue venue before loading an engine', () => {
    // restored by #3502; #3317 dropped a live venue without a product decision.
    expect(BETA_VENUES).toContain('bitrue');
    // restored by #3502; #3317 dropped a live venue without a product decision.
    expect(() => assertBetaAccount('bitrue', { key: '', secret: '', publicDataOnly: true })).not.toThrow();
  });
  it('has exactly the declared IN names', () => {
    expect([...BETA_VENUES]).toEqual(expected);
    expect(getRegisteredExchangeNames()).toEqual(expected);
    // Robinhood now uses the same beta runtime as Lighter.
    // restored by #3502; #3317 dropped a live venue without a product decision.
    expect(new Set(BETA_VENUES).size).toBe(44);
    // restored by #3502; #3317 dropped a live venue without a product decision.
    expect(out).not.toContain('bitrue');
    expect(out).toContain('polymarket');
  });
  it('constructs Rust for every IN name without an engine opt-in', async () => {
    for (const name of BETA_VENUES) {
      expect(await exchangeRegistry[name]()).toBe(state.Rust);
      expect(await createExchange(name, { key: '', secret: '', publicDataOnly: true })).toBeInstanceOf(state.Rust);
    }
    // restored by #3502; #3317 dropped a live venue without a product decision.
    expect(state.calls).toHaveLength(44);
    expect(state.calls.every(({ options }) => (options as { engine: string }).engine === 'rust')).toBe(true);
  });
  it('refuses OUT web/native loaders before loading a runtime', async () => {
    const load = vi.fn(async () => state.Rust);
    const registry = createRustRegistry(load as unknown as Parameters<typeof createRustRegistry>[0]);
    for (const name of out) {
      await expect(registry[name]()).rejects.toMatchObject(unsupported);
      await expect(exchangeRegistry[name]()).rejects.toBeInstanceOf(ExchangeError);
      await expect(nativeRegistry[name]()).rejects.toMatchObject(unsupported);
    }
    expect(load).not.toHaveBeenCalled();
  });
  it('refuses OUT factories before clock or constructor work', async () => {
    for (const name of out) {
      await expect(createExchange(name, { key: '', secret: '' })).rejects.toMatchObject(unsupported);
      await expect(createPublicExchange(name)).rejects.toMatchObject(unsupported);
      await expect(preloadExchange(name)).rejects.toMatchObject(unsupported);
      await expect(createExchangeFromAccount({ exchange: name } as ExchangeAccountConfig)).rejects.toMatchObject(
        unsupported,
      );
    }
    await expect(createExchange('unknown' as ExchangeName, { key: '', secret: '' })).rejects.toMatchObject(unsupported);
    expect(virtualClock.start).not.toHaveBeenCalled();
    expect(state.calls).toEqual([]);
  });
  it('guards the actual direct adapter constructor before WASM loading', async () => {
    const { RustEngineExchange, setWasmEngineLoader } = await vi.importActual<
      typeof import('../../exchanges/rust/rust.exchange')
    >('../../exchanges/rust/rust.exchange');
    const load = vi.fn();
    setWasmEngineLoader(load);
    for (const name of out) {
      expect(
        () => new RustEngineExchange(name, { key: '', secret: '', publicDataOnly: true }, new DefaultStore()),
      ).toThrow('unsupported in this build');
    }
    const working = new RustEngineExchange('bybit', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    working.dispose();
    expect(load).not.toHaveBeenCalled();
  });
  it('worker startup refuses OUT before clock or runtime construction', async () => {
    const { ExchangeWorker } = await import('../../webworker/ExchangeWorker');
    const messages: unknown[] = [];
    const worker = new ExchangeWorker(
      { postMessage: (message: unknown) => messages.push(message) } as unknown as Worker,
      'saved-out-account',
      'polymarket',
      { key: '', secret: '' },
      {} as never,
      {} as never,
      'session',
    );
    await expect(worker.start()).resolves.toBe(false);
    expect(virtualClock.start).not.toHaveBeenCalled();
    expect(state.calls).toEqual([]);
    expect(JSON.stringify(messages)).toContain('polymarket is unsupported in this build');
    worker.dispose();
  });
  it('admits Robinhood saved-account construction before key validation', async () => {
    const account: ExchangeAccountConfig = {
      exchange: 'lighter-robinhood',
      name: 'robinhood',
      apiKey: '1',
      apiSecret: `0x${'22'.repeat(40)}`,
    };
    await expect(createExchangeFromAccount(account)).resolves.toBeInstanceOf(state.Rust);
    expect(state.calls[0]?.name).toBe('lighter-robinhood');
  });
  it('admits Robinhood Hosted through the existing Lighter remote-signing policy', async () => {
    const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
    const options = { key: '1', secret: '', apiKeyIndex: '2', remoteSigner };
    expect(deriveRemoteSigningPolicy('lighter-robinhood')?.enabled).toBe(true);
    expect(() => assertBetaAccount('lighter-robinhood', options)).not.toThrow();
    await expect(createExchange('lighter-robinhood', options)).resolves.toBeInstanceOf(state.Rust);
  });
  it.each(['remoteSigner', 'payloadSigner'] as const)(
    'explains refused %s accounts before clock or constructor work',
    async (signerKind) => {
      const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
      const payloadSigner = vi.fn();
      const signing = signerKind === 'remoteSigner' ? { remoteSigner } : { payloadSigner };
      for (const name of BETA_VENUES.filter((venue) => deriveRemoteSigningPolicy(venue)?.enabled !== true)) {
        const prettyName = exchangeMetadata[name].prettyName;
        const message = signerKind === 'remoteSigner'
          ? `${prettyName} isn't available on the Hosted Server yet; turn Hosted Server off to trade it with your API key in this browser`
          : `${prettyName} delegated signing is unsupported in this build`;
        await expect(createExchange(name, { key: 'public-key', secret: '', ...signing })).rejects.toMatchObject({
          message,
          kind: 'unsupported',
          outcome: 'notSent',
        });
      }
      await expect(createExchange('unknown' as ExchangeName, { key: '', secret: '' })).rejects.toMatchObject({
        message: 'unknown is unsupported in this build',
        kind: 'unsupported',
        outcome: 'notSent',
      });
      expect(remoteSigner.sign).not.toHaveBeenCalled();
      expect(payloadSigner).not.toHaveBeenCalled();
      expect(virtualClock.start).not.toHaveBeenCalled();
      expect(state.calls).toEqual([]);
    },
  );
  it('refuses unsupported delegated modes without fallback', async () => {
    const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
    // Hyperliquid's, Lighter's and Paradex's structured remote signing is pinned in
    // their rust.*-remote-signing.wasm.test.ts.
    const localOnly = ['krakenspot'] as const;
    // #2262 enables Bitunix through the shared policy, without a beta override.
    const refused: readonly ExchangeName[] = deriveRemoteSigningPolicy('bitunix')?.enabled
      ? localOnly
      : [...localOnly, 'bitunix'];
    for (const name of refused) {
      for (const signing of [{ remoteSigner }, { payloadSigner: vi.fn() }]) {
        expect(() => assertBetaAccount(name, { key: 'local', secret: 'local', ...signing })).toThrow(ExchangeError);
        await expect(createExchange(name, { key: 'local', secret: 'local', ...signing })).rejects.toMatchObject(
          unsupported,
        );
      }
    }
    expect(virtualClock.start).not.toHaveBeenCalled();
    expect(state.calls).toEqual([]);
  });
});
