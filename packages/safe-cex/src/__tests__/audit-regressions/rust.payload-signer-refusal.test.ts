import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import {
  runsOnRustEngine,
  RustEngineExchange,
  setRustEngineTransport,
  setWasmEngineLoader,
} from '../../exchanges/rust/rust.exchange';
import { isKeylessRemoteSigned } from '../../exchanges/hostedSigningVenues';
import { createExchange } from '../../index';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';
import { engineCapabilities } from '../fixtures/engineCapabilities';

const running: RustEngineExchange[] = [];
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
});

const payloadSigner = vi.fn(async () => ({ signature: 'never-requested' }));

const recordEngine = (name: string) => {
  const construct = vi.fn();
  class Engine {
    constructor(venue: string, options: string) {
      construct(venue, JSON.parse(options));
    }
    capabilities() {
      return engineCapabilities(name);
    }
    free() {}
  }
  setWasmEngineLoader(async () => ({ WasmEngine: Engine }) as unknown as WasmEngineModule);
  const transport = vi.fn(() => {
    throw new Error('transport must not start');
  });
  setRustEngineTransport(transport as never);
  return { construct, transport };
};

const start = (name: ExchangeName, options: Partial<ExchangeOptions>) => {
  const exchange = new RustEngineExchange(
    name,
    { key: 'k', secret: '', apiKeyIndex: 2, ...options } as ExchangeOptions,
    new DefaultStore(),
  );
  running.push(exchange);
  return (exchange as unknown as { ensureHost(): Promise<void> }).ensureHost();
};

// MCP signer mode supplies a payloadSigner and no secret. These venues skip the
// generic local-credential guard, so before this refusal they started unsigned.
it.each([
  'hyperliquid',
  'txflow',
  'coinbase',
  'coinbasespot',
  'polymarket',
  'lighter',
  'lighter-robinhood',
  'binance',
  'bybit',
] as const)('refuses a payloadSigner on private %s before the engine or transport starts', async (name) => {
  // Lighter signs locally, and a payload-only Lighter account starts public by contract.
  for (const secret of name.startsWith('lighter') ? ['local-secret'] : ['', 'local-secret']) {
    const { construct, transport } = recordEngine(name);
    const failure = await start(name, { secret, payloadSigner }).catch((error: unknown) => error);
    expect(failure).toMatchObject({
      kind: 'unsupported',
      outcome: 'notSent',
      message: expect.stringContaining(`Rust ${name} does not support payloadSigner`),
    });
    expect((failure as Error).message).toContain('remote signing is unsupported');
    expect(construct).not.toHaveBeenCalled();
    expect(transport).not.toHaveBeenCalled();
  }
  expect(payloadSigner).not.toHaveBeenCalled();
});

it('keeps Paradex delegated signing and explicitly public-only accounts', async () => {
  for (const [name, options] of [
    ['paradex', { payloadSigner, paradexAddress: '0x1', subkeyAddress: '0x2' }],
    ['bybit', { payloadSigner, publicDataOnly: true }],
  ] as const) {
    const { transport } = recordEngine(name);
    // The recording transport throws once reached, which proves startup passed the refusal.
    const failure = await start(name, options as Partial<ExchangeOptions>).catch((error: unknown) => error);
    expect(String((failure as Error)?.message)).not.toContain('does not support payloadSigner');
    expect(transport).toHaveBeenCalled();
  }
});

it('reports the structured refusal through start and validateAccount', async () => {
  const { construct, transport } = recordEngine('hyperliquid');
  const hyperliquid = new RustEngineExchange(
    'hyperliquid',
    { key: 'k', secret: '', payloadSigner } as ExchangeOptions,
    new DefaultStore(),
  );
  running.push(hyperliquid);
  await expect(hyperliquid.start()).rejects.toMatchObject({ kind: 'unsupported', outcome: 'notSent' });

  const coinbase = new RustEngineExchange(
    'coinbase',
    { key: 'k', secret: '', payloadSigner } as ExchangeOptions,
    new DefaultStore(),
  );
  running.push(coinbase);
  await expect(coinbase.validateAccount()).resolves.toMatchObject({
    kind: 'unsupported',
    error: expect.stringContaining('Rust coinbase does not support payloadSigner'),
  });
  expect(construct).not.toHaveBeenCalled();
  expect(transport).not.toHaveBeenCalled();
});

it('routes only TS venues that sign with a payload signer to TS', () => {
  for (const name of ['bybit', 'bybitspot', 'hyrotrader', 'binance', 'binancepm', 'binanceinverse'] as const) {
    expect(runsOnRustEngine(name, { engine: 'rust', secret: '', payloadSigner }), name).toBe(false);
    expect(runsOnRustEngine(name, { engine: 'rust', secret: 's', payloadSigner }), name).toBe(false);
    expect(runsOnRustEngine(name, { engine: 'rust', payloadSigner, publicDataOnly: true }), name).toBe(true);
    expect(runsOnRustEngine(name, { engine: 'rust', secret: 's' }), name).toBe(true);
  }
  // Their TS venues ignore a payload signer too, so the engine's startup refusal is the outcome.
  for (const name of ['hyperliquid', 'coinbase', 'polymarket', 'lighter', 'paradex'] as const) {
    expect(runsOnRustEngine(name, { engine: 'rust', secret: '', payloadSigner }), name).toBe(true);
  }
});

describe('factory and worker loaders', () => {
  afterEach(() => vi.restoreAllMocks());
  const signed = (extra: Partial<ExchangeOptions> = {}): ExchangeOptions =>
    ({ key: 'k', secret: '', payloadSigner, engine: 'rust', ...extra }) as ExchangeOptions;

  it('createExchange builds the TS venue for Bybit and Binance payload signers', async () => {
    vi.spyOn(virtualClock, 'start').mockResolvedValue(undefined as never);
    for (const name of ['bybit', 'binance'] as const) {
      const exchange = await createExchange(name, signed());
      try {
        expect(exchange, name).not.toBeInstanceOf(RustEngineExchange);
      } finally {
        exchange.dispose();
      }
    }
  }, 60_000);

  it('a default-Rust name never falls back to its hosted TS class, which cannot payload-sign', async () => {
    vi.spyOn(virtualClock, 'start').mockResolvedValue(undefined as never);
    for (const name of ['coinbase', 'tealstreet-dex'] as const) {
      // The hosted loader serves keyless remote signing only.
      expect(isKeylessRemoteSigned(name, signed()), name).toBe(false);
      for (const options of [signed(), signed({ engine: undefined })]) {
        recordEngine(name);
        const exchange = await createExchange(name, options);
        running.push(exchange as RustEngineExchange);
        expect(exchange, name).toBeInstanceOf(RustEngineExchange);
        await expect(exchange.start(), name).rejects.toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
      }
    }
  }, 60_000);
});
