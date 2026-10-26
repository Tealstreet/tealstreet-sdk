vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { ExchangeName } from '../../types';

import { expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

it.each<ExchangeName>(['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'])(
  '%s cancel-all sends the unscoped native cancel instead of looping stored symbols',
  async (name) => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange(name, { key: 'fixture-key', secret: 'fixture-secret' }, store);
    adoptEngineCapabilities(exchange);
    const boundary = exchange as unknown as { command: (input: unknown) => Promise<string[]> };
    const command = vi.spyOn(boundary, 'command').mockResolvedValue([]);
    try {
      await exchange.cancelAllOrders();
      expect(command.mock.calls).toEqual([[{ op: 'cancelSymbolOrders', symbol: '' }]]);
    } finally {
      exchange.dispose();
    }
  },
);
