import type { ExchangeAccountConfig, ExchangeLike } from '@tealstreet/cli-core';

import { describe, expect, it, vi } from 'vitest';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { ExchangeManager } from './exchange-manager';
import { createStandalonePlatformAdapter } from './platform-adapter';

// BitMEX closed 2026-09-23; users still have its accounts saved in config.json.
const account = (exchange: string, name = `${exchange}-main`) =>
  ({ name, exchange, key: 'saved-key', secret: 'saved-secret', active: true }) as unknown as ExchangeAccountConfig;

describe('removed venues on saved CLI accounts', () => {
  // breakoutprop was removed after bitmex (#3224); it once kept tunnel-enforcing metadata, so the refusal must precede login checks.
  it.each(['bitmex', 'bitmextestnet', 'breakoutprop'])(
    'refuses %s typed before login or network work',
    async (exchange) => {
      const adapter = createStandalonePlatformAdapter();
      const error = await adapter.createExchange(account(exchange)).catch((caught: unknown) => caught);
      expect(error).toBeInstanceOf(ExchangeError);
      expect(error).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
    },
  );

  it('leaves the other saved accounts enabling', async () => {
    const real = createStandalonePlatformAdapter();
    const live = { name: 'live' } as unknown as ExchangeLike;
    const manager = new ExchangeManager({
      ...real,
      createExchange: (saved: ExchangeAccountConfig) =>
        saved.exchange === 'binance' ? Promise.resolve(live) : real.createExchange(saved),
      startExchange: vi.fn(async () => undefined),
      updateAccount: vi.fn(),
    });
    const results = await Promise.allSettled(
      [account('bitmex'), account('binance'), account('bitmextestnet'), account('breakoutprop')].map((saved) =>
        manager.enable(saved),
      ),
    );
    expect(results.map((result) => result.status)).toEqual(['rejected', 'fulfilled', 'rejected', 'rejected']);
    for (const index of [0, 2, 3]) {
      expect((results[index] as PromiseRejectedResult).reason).toMatchObject({
        kind: 'unsupported',
        outcome: 'notSent',
      });
    }
    expect(manager.isConnected('binance-main')).toBe(true);
    expect(manager.isConnected('bitmex-main')).toBe(false);
    expect(manager.isConnected('bitmextestnet-main')).toBe(false);
    expect(manager.isConnected('breakoutprop-main')).toBe(false);
  });
});
