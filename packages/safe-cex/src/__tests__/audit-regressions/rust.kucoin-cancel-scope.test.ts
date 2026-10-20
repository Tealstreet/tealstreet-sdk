import { expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

it('KuCoin SDK symbol cancellation preserves stops while true cancel-all reaches both books', async () => {
  const exchange = new RustEngineExchange('kucoin', { key: 'fixture-key', secret: 'fixture-secret', passphrase: 'fixture-pass' }, new DefaultStore());
  const boundary = exchange as unknown as { command: (input: unknown) => Promise<string[]> };
  const command = vi.spyOn(boundary, 'command').mockResolvedValue([]);
  try {
    await exchange.cancelSymbolOrders('BTCUSDT');
    await exchange.cancelAllOrders();
    expect(command.mock.calls).toEqual([
      [{ op: 'cancelSymbolOrders', symbol: 'BTCUSDT' }],
      [{ op: 'cancelSymbolOrders', symbol: '' }],
    ]);
  } finally {
    exchange.dispose();
  }
});
