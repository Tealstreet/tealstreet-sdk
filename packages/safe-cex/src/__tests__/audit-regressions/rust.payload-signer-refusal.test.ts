import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import { afterEach, expect, it, vi } from 'vitest';
import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import type { ExchangeName, ExchangeOptions } from '../../types';
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
  for (const secret of ['', 'local-secret']) {
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
