vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { ExchangeName } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

// Symbol switches and chaser reconciliation read in the background. Their callers
// own visibility (a warning, or the chaser's own error), so the command boundary
// must not add an error toast per failed read.
const accounts: RustEngineExchange[] = [];
afterEach(() => accounts.splice(0).forEach((exchange) => exchange.dispose()));

const rejecting = (name: ExchangeName, passphrase?: string) => {
  const exchange = new RustEngineExchange(name, { key: 'key', secret: 'secret', passphrase }, new DefaultStore());
  accounts.push(exchange);
  const command = vi.fn().mockRejectedValue(new Error('leverage unavailable'));
  (exchange as unknown as { host: unknown }).host = { command, flush: vi.fn(), dispose: vi.fn() };
  adoptEngineCapabilities(exchange);
  const errors = vi.fn();
  exchange.on('error', errors);
  return { exchange, command, errors };
};

it('keeps a failed symbol-switch leverage read at warning level', async () => {
  const { exchange, command, errors } = rejecting('blofin', 'pass');
  const warnings = vi.fn();
  exchange.on('warning', warnings);
  await expect(exchange.fetchLeverageForSymbol('BTCUSDT')).resolves.toBeNull();
  expect(command).toHaveBeenCalledWith({ op: 'fetchLeverage', symbol: 'BTCUSDT' });
  expect(errors).not.toHaveBeenCalled();
});

it('keeps a failed symbol metadata refresh at warning level, with no rejection for the worker', async () => {
  const mexc = rejecting('mexc');
  const warnings = vi.fn();
  mexc.exchange.on('warning', warnings);
  await expect(mexc.exchange.updateSymbolMetadata('BTCUSDT')).resolves.toBeUndefined();
  expect(warnings).toHaveBeenCalledTimes(1);
  expect(mexc.errors).not.toHaveBeenCalled();
});

it('lets chaser fill reads reject without an error event', async () => {
  const grvt = rejecting('grvt');
  await expect(grvt.exchange.fetchFillsForOrderIds!(['order-1'])).rejects.toThrow('leverage unavailable');
  expect(grvt.errors).not.toHaveBeenCalled();
});

it('still reports a failed write once', async () => {
  const { exchange, errors } = rejecting('blofin', 'pass');
  await expect(exchange.setLeverage('BTCUSDT', 3)).rejects.toThrow('leverage unavailable');
  expect(errors).toHaveBeenCalledTimes(1);
});
