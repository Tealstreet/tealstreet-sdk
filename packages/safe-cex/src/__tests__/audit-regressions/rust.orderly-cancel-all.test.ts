import type { ExchangeName } from '../../types';

import { expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

it.each<ExchangeName>(['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'])(
  '%s cancel-all sends the unscoped native cancel instead of looping stored symbols',
  async (name) => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange(name, { key: 'fixture-key', secret: 'fixture-secret' }, store);
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
