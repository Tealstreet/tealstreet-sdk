import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';
import { expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

it('Binance USD-M journal folds per-symbol fills like the TS venue', async () => {
  const exchange = new RustEngineExchange('binance', { key: 'k', secret: 's' }, new DefaultStore());
  adoptEngineCapabilities(exchange);
  const fold = vi
    .spyOn(exchange as unknown as { foldClosedPositionRecords: (...args: unknown[]) => Promise<unknown[]> }, 'foldClosedPositionRecords')
    .mockResolvedValue([]);
  try {
    await expect(exchange.fetchHistoricalPositions({ account: 'acct', from: 1, to: 2 })).resolves.toEqual([]);
    expect(fold).toHaveBeenCalledWith({ account: 'acct', from: 1, to: 2 }, { symbolScoped: true });
  } finally {
    exchange.dispose();
  }
});
