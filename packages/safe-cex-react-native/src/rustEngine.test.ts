import type { WasmEngineModule } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import type { VenueFamily } from '@tealstreet/safe-cex-wasm/venues';

import { beforeEach, expect, it, vi } from 'vitest';

import { setWasmEngineLoader } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import { setReactNativeRustEngineLoader } from './rustEngine';

vi.mock('@tealstreet/safe-cex/src/exchanges/rust/rust.exchange', () => ({ setWasmEngineLoader: vi.fn() }));
beforeEach(() => vi.clearAllMocks());
const moduleFor = (name: string) => ({ name }) as unknown as WasmEngineModule;
const registered = () => vi.mocked(setWasmEngineLoader).mock.calls[0][0];

it('shares concurrent aliases while loading different families independently', async () => {
  const coinbase = moduleFor('coinbase');
  const bybit = moduleFor('bybit');
  const loader = vi.fn(async (family: VenueFamily) => (family === 'coinbase' ? coinbase : bybit));
  await setReactNativeRustEngineLoader(loader);
  const load = registered();
  const first = load('coinbase');
  expect(load('coinbasespot')).toBe(first);
  const other = load('bybitdemo');
  expect(other).not.toBe(first);
  expect(await first).toBe(coinbase);
  expect(await other).toBe(bybit);
  expect(loader.mock.calls).toEqual([['coinbase'], ['bybit']]);
});

it('retries a failed family without resetting another family or duplicating concurrent loads', async () => {
  const bybit = moduleFor('bybit');
  const coinbase = moduleFor('coinbase');
  let attempts = 0;
  const loader = vi.fn(async (family: VenueFamily) => {
    if (family === 'bybit') return bybit;
    if (++attempts === 1) throw new Error('host initialization failed');
    return coinbase;
  });
  await setReactNativeRustEngineLoader(loader);
  const load = registered();
  const other = load('bybit');
  const failed = load('coinbase');
  expect(load('coinbasespot')).toBe(failed);
  await expect(failed).rejects.toThrow('host initialization failed');
  expect(await other).toBe(bybit);
  const retry = load('coinbasespot');
  expect(load('coinbase')).toBe(retry);
  expect(load('bybitdemo')).toBe(other);
  expect(await retry).toBe(coinbase);
  expect(loader.mock.calls).toEqual([['bybit'], ['coinbase'], ['coinbase']]);
});

it('rejects unsupported venue names before calling the host loader', async () => {
  const loader = vi.fn(async () => moduleFor('unused'));
  await setReactNativeRustEngineLoader(loader);
  expect(() => registered()('unknown')).toThrow('unsupported Rust venue');
  expect(loader).not.toHaveBeenCalled();
});
