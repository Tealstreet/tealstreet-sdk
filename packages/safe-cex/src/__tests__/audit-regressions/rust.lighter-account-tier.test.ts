vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { ExchangeName } from '../../types';

import { expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

it.each<ExchangeName>(['lighter', 'lighter-robinhood'])(
  '%s exposes the app tier switch and the cached tier through engine venue actions',
  async (name) => {
    const exchange = new RustEngineExchange(name, { key: 'fixture-key', secret: 'fixture-secret' }, new DefaultStore());
    adoptEngineCapabilities(exchange);
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
  adoptEngineCapabilities(exchange);
  try {
    expect(exchange.changeAccountTier).toBeUndefined();
  } finally {
    exchange.dispose();
  }
});

it('the tier reaches balance.info.lighterAccountLimits in the TS shape the settings page reads', async () => {
  const store = new DefaultStore();
  const exchange = new RustEngineExchange('lighter', { key: 'fixture-key', secret: 'fixture-secret' }, store);
  adoptEngineCapabilities(exchange);
  const boundary = exchange as unknown as {
    command: (input: unknown) => Promise<unknown>;
    syncLighterTier: () => Promise<void>;
  };
  vi.spyOn(boundary, 'command').mockResolvedValueOnce('standard').mockResolvedValueOnce(null);
  try {
    store.update({ balance: { used: 1, free: 2, total: 3, upnl: 0 } });
    await boundary.syncLighterTier();
    expect(store.balance.info.lighterAccountLimits).toEqual({ user_tier_name: 'standard' });
    expect(store.balance.total).toBe(3);
    await exchange.changeAccountTier!('plus');
    expect(store.balance.info.lighterAccountLimits).toEqual({ user_tier_name: 'plus' });
  } finally {
    exchange.dispose();
  }
});
