vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import { afterEach, expect, it, vi } from 'vitest';
import { LOADED_FLAG } from '@tealstreet/safe-cex-wasm/wire';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const exchanges: RustEngineExchange[] = [];
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

const setup = (nativeLoaded: number) => {
  const exchange = new RustEngineExchange('phemex', { key: '', secret: '', publicDataOnly: true } as never, new DefaultStore());
  exchanges.push(exchange);
  const command = vi.fn().mockResolvedValue([]);
  Object.defineProperty(exchange, 'command', { value: command });
  Object.defineProperty(exchange, 'nativeLoaded', { value: nativeLoaded, writable: true });
  Object.defineProperty(exchange, 'start', { value: async () => undefined });
  return { exchange, command };
};
const candles = { symbol: 'U1000PEPEUSDT', interval: '30m', limit: 2 } as never;

// Before its market list an engine refuses or sends the wrong id: Market not found on Lighter
// and Hyperliquid, Unknown candle market on Paradex, 30008 on Phemex's unmapped U1000PEPEUSDT.
it('holds a candle read until the engine has its markets', async () => {
  const { exchange, command } = setup(0);
  const read = exchange.fetchOHLCV(candles);
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(command).not.toHaveBeenCalled();
  (exchange as unknown as { nativeLoaded: number }).nativeLoaded = LOADED_FLAG.MARKETS;
  for (const wake of (exchange as unknown as { startupWaiters: Set<() => void> }).startupWaiters) wake();
  await read;
  expect(command).toHaveBeenCalledTimes(1);
});

it('reads at once when the markets are in, even if the start later failed', async () => {
  const { exchange, command } = setup(LOADED_FLAG.MARKETS);
  (exchange as unknown as { startupFailure: Error }).startupFailure = new Error('key refused');
  await exchange.fetchOHLCV(candles);
  expect(command).toHaveBeenCalledTimes(1);
});
