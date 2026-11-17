import type { ExchangeName } from '../../types';

import { expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { FrameStore } from '../../store/frameStore';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

it.each<ExchangeName>(['kodiakfi', 'mode', 'what', 'woofi'])(
  '%s cancel-all sends the unscoped native cancel instead of looping stored symbols',
  async (name) => {
    const store = new FrameStore();
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
