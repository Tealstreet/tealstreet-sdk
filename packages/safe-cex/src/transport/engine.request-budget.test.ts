import type { Transport, HttpPacket } from '@tealstreet/safe-cex-wasm/host';
import { afterEach, expect, it, vi } from 'vitest';
import { createEngineTransport } from './engine';
import { directTransport } from './direct';

vi.mock('../index', () => ({ createExchange: vi.fn() }));
afterEach(() => vi.useRealTimers());

it.each([
  ['discovery', 60_000], ['history', 30_000], ['orderRead', 20_000],
  ['orderWrite', 20_000], ['read', 20_000],
] as const)('engine transport bounds a stalled %s preparation', async (requestClass, budgetMs) => {
  vi.useFakeTimers(); vi.setSystemTime(0);
  const raw = {
    now: () => Date.now(),
    request: vi.fn(),
    prepareHttp: vi.fn(() => new Promise(() => {})),
    connect: vi.fn(), setTimeout, clearTimeout,
  } as unknown as Transport;
  const transport = createEngineTransport('bybit', { key: '', secret: '', transport: directTransport }, raw);
  let error: any;
  try {
    const pending = Promise.resolve(transport.prepareHttp!({ request: 1, method: 'POST', url: 'https://venue.test/private', headers: {}, intent: 'primary', requestClass } as HttpPacket & { intent: 'primary' }))
      .catch(e => { error = e; });
    await vi.advanceTimersByTimeAsync(budgetMs - 1);
    expect(error).toBeUndefined();
    await vi.advanceTimersByTimeAsync(1); await pending;
    expect(error).toMatchObject({ code: 'HTTP_PREPARATION_TIMED_OUT', requestClass, budgetMs, elapsedMs: budgetMs });
    expect(error.message).toContain(`class=${requestClass} budget=${budgetMs}ms elapsed=${budgetMs}ms`);
    expect(raw.request).not.toHaveBeenCalled();
  } finally { transport.dispose?.(); }
});
