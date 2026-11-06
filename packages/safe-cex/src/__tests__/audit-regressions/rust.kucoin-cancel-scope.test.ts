vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import { expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

it('KuCoin SDK symbol cancellation preserves stops while true cancel-all reaches both books', async () => {
  const exchange = new RustEngineExchange('kucoin', { key: 'fixture-key', secret: 'fixture-secret', passphrase: 'fixture-pass' }, new DefaultStore());
  const boundary = exchange as unknown as { command: (input: unknown) => Promise<string[]> };
  const command = vi.spyOn(boundary, 'command').mockResolvedValue([]);
  adoptEngineCapabilities(exchange);
  try {
    await exchange.cancelSymbolOrders('BTCUSDT');
    await exchange.cancelAllOrders();
    expect(command.mock.calls.map(([input]) => input)).toEqual([
      { op: 'cancelSymbolOrders', symbol: 'BTCUSDT' },
      { op: 'cancelSymbolOrders', symbol: '' },
    ]);
  } finally {
    exchange.dispose();
  }
});
