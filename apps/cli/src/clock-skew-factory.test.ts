import { expect, it, vi } from 'vitest';

import { createExchangeFromConfig } from './exchange-factory';

vi.mock('../../../packages/safe-cex/src/utils/virtualClock', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../packages/safe-cex/src/utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: vi.fn(async () => undefined) } };
});

it('the real CLI account factory prints a typed correction once, with no duplicate adapter log', async () => {
  const exchange = await createExchangeFromConfig({
    name: 'fixture',
    exchange: 'bybit',
    apiKey: 'fixture-key',
    apiSecret: 'fixture-secret',
  } as any);
  const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
  const log = vi.spyOn(exchange, 'log').mockImplementation(() => {});
  try {
    for (let i = 0; i < 3; i++) {
      (exchange as any).receiveEvent({
        code: 'bybit.clock.corrected',
        level: 'info',
        message: 'Bybit protocol clock corrected',
        fields: { offsetMs: -713451 },
      });
    }
    expect(warning).toHaveBeenCalledOnce();
    expect(warning.mock.calls[0][0]).toContain('11.9 min fast; sync your system time');
    expect(log).not.toHaveBeenCalled();
  } finally {
    exchange.dispose();
    warning.mockRestore();
  }
});
