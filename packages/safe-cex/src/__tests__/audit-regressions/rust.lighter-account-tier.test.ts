import type { ExchangeName } from '../../types';

import { expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

it.each<ExchangeName>(['lighter', 'lighter-robinhood'])(
  '%s exposes the app tier switch and the cached tier through engine venue actions',
  async (name) => {
    const exchange = new RustEngineExchange(name, { key: 'fixture-key', secret: 'fixture-secret' }, new DefaultStore());
    const boundary = exchange as unknown as { command: (input: unknown) => Promise<unknown> };
    const command = vi.spyOn(boundary, 'command').mockResolvedValueOnce(null).mockResolvedValueOnce('premium');
    try {
      await exchange.changeAccountTier!('premium');
      await expect(exchange.fetchAccountTier!()).resolves.toBe('premium');
      expect(command.mock.calls).toEqual([
        [{ op: 'setAccountTier', tier: 'premium' }],
        [{ op: 'fetchAccountTier' }],
      ]);
    } finally {
      exchange.dispose();
    }
  },
);

it('other Rust venues do not advertise a tier switch', () => {
  const exchange = new RustEngineExchange('binance', { key: 'k', secret: 's' }, new DefaultStore());
  try {
    expect(exchange.changeAccountTier).toBeUndefined();
  } finally {
    exchange.dispose();
  }
});
