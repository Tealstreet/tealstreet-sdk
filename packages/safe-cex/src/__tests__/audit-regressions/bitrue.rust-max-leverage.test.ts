vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { Market } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';
import { LOADED_FLAG } from '@tealstreet/safe-cex-wasm/wire';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

const exchanges: RustEngineExchange[] = [];
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

const setup = (publicDataOnly = false) => {
  const store = new DefaultStore();
  store.update({ markets: [{
    id: 'E-BTC-USDT', symbol: 'E-BTC-USDT', base: 'BTC', quote: 'USDT', active: true,
    precision: { amount: 0.001, price: 0.1 },
    limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } }, info: {},
  } satisfies Market] });
  const exchange = new RustEngineExchange('bitrue', { key: 'replay-key', secret: 'replay-secret', publicDataOnly }, store);
  exchanges.push(exchange);
  const command = vi.fn().mockResolvedValue(125);
  Object.defineProperty(exchange, 'command', { value: command });
  adoptEngineCapabilities(exchange);
  // The metadata refresh waits for the engine's own markets; no engine starts here.
  Object.defineProperty(exchange, 'nativeLoaded', { value: LOADED_FLAG.MARKETS, writable: true });
  Object.defineProperty(exchange, 'start', { value: async () => undefined });
  return { exchange, store, command };
};

it('reads the native bracket rather than the public 100x fallback without inventing account leverage', async () => {
  const { exchange, store, command } = setup();
  await expect(exchange.fetchSymbolMaxLeverage('E-BTC-USDT')).resolves.toBe(125);
  expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'fetchMaxLeverage', symbol: 'E-BTC-USDT' });
  expect(store.fetchedLeverage['E-BTC-USDT']).toBeUndefined();
});

it('refreshes bracket metadata on a fresh flat account without requiring current leverage', async () => {
  const { exchange, command } = setup();
  await exchange.updateSymbolMetadata('E-BTC-USDT');
  expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'fetchMaxLeverage', symbol: 'E-BTC-USDT' });
});

it('keeps public-only metadata reads free of authenticated bracket requests', async () => {
  const { exchange, command } = setup(true);
  await exchange.updateSymbolMetadata('E-BTC-USDT');
  await expect(exchange.fetchSymbolMaxLeverage('E-BTC-USDT')).resolves.toBe(100);
  expect(command).not.toHaveBeenCalled();
});

it('surfaces a failed bracket read rather than returning an invented ceiling', async () => {
  const { exchange, command } = setup();
  command.mockRejectedValue(new Error('Bitrue bracket unavailable'));
  await expect(exchange.fetchSymbolMaxLeverage('E-BTC-USDT')).rejects.toThrow('bracket unavailable');
});

it.each([0, Number.NaN, Number.POSITIVE_INFINITY])('rejects malformed maximum leverage %s', async (maximum) => {
  const { exchange, command } = setup();
  command.mockResolvedValue(maximum);
  await expect(exchange.fetchSymbolMaxLeverage('E-BTC-USDT')).rejects.toThrow('Invalid Bitrue maximum leverage');
});
