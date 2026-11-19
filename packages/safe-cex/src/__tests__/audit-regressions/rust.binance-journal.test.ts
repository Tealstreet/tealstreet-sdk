import { expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { FrameStore } from '../../store/frameStore';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

it('Binance USD-M journal folds per-symbol fills like the TS venue', async () => {
  const exchange = new RustEngineExchange('binance', { key: 'k', secret: 's' }, new FrameStore());
  adoptEngineCapabilities(exchange);
  const fold = vi
    .spyOn(
      exchange as unknown as { foldClosedPositionRecords: (...args: unknown[]) => Promise<unknown[]> },
      'foldClosedPositionRecords',
    )
    .mockResolvedValue([]);
  try {
    await expect(exchange.fetchHistoricalPositions({ account: 'acct', from: 1, to: 2 })).resolves.toEqual([]);
    expect(fold).toHaveBeenCalledWith({ account: 'acct', from: 1, to: 2 }, { symbolScoped: true });
  } finally {
    exchange.dispose();
  }
});
