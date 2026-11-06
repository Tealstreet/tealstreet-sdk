import axios from 'axios';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { AsterDEXExchange } from '../reference/asterdex/asterdex.exchange';
import { ASTERDEX_ENDPOINTS } from '../../exchanges/asterdex/asterdex.types';
import { asterDEXVirtualClock } from '../../exchanges/asterdex/asterdex.virtualClock';
import { DefaultStore } from '../../store/store.base';

vi.mock('axios', async () => {
  const actual = await vi.importActual<typeof import('axios')>('axios');
  return {
    ...actual,
    default: { ...actual.default, get: vi.fn() },
  };
});

afterEach(() => {
  asterDEXVirtualClock.stop();
  vi.restoreAllMocks();
});

describe('audit regression: AsterDEX testnet endpoints and worker clock', () => {
  it('uses the documented testnet REST and public/private WebSocket hosts', async () => {
    expect(ASTERDEX_ENDPOINTS.REST.TESTNET).toBe('https://fapi.asterdex-testnet.com');
    expect(ASTERDEX_ENDPOINTS.WS.PUBLIC.TESTNET).toBe('wss://fstream5.asterdex-testnet.com/stream');
    expect(ASTERDEX_ENDPOINTS.WS.PRIVATE.TESTNET).toBe('wss://fstream5.asterdex-testnet.com/stream');
    expect(JSON.stringify(ASTERDEX_ENDPOINTS)).not.toContain('testnet-fapi.asterdex.com');

    const exchange = new AsterDEXExchange(
      'asterdex',
      { key: 'test-key', secret: 'test-secret', testnet: true },
      new DefaultStore(),
    );
    expect(exchange.publicWebsocket.connectionURL).toMatch(/^wss:\/\/fstream5\.asterdex-testnet\.com\/stream\?/);

    const privateWebsocket = exchange.privateWebsocket as any;
    const createWebsocket = vi.fn(() => ({ addEventListener: vi.fn() }));
    privateWebsocket.createWebsocket = createWebsocket;
    exchange.primaryAPI.post = vi.fn().mockResolvedValue({ data: { listenKey: 'test-listen-key' } });

    await privateWebsocket.connectAndSubscribe();

    expect(createWebsocket).toHaveBeenCalledWith('wss://fstream5.asterdex-testnet.com/ws/test-listen-key');
    privateWebsocket.dispose();
  });

  it('synchronizes from Aster in a non-window worker runtime and preserves a good offset after a failed retry', async () => {
    const originalWindow = global.window;
    (global as any).window = undefined;
    const serverTime = Date.now() + 7_000;
    vi.mocked(axios.get).mockResolvedValueOnce({ data: { serverTime } } as any);

    await asterDEXVirtualClock.syncClientServerTime(true, 'v3');

    expect(axios.get).toHaveBeenCalledWith('https://fapi.asterdex-testnet.com/fapi/v3/time', { timeout: 5000 });
    expect(asterDEXVirtualClock.getTimeOffset()).toBeGreaterThan(6_500);
    const offsetBeforeFailure = asterDEXVirtualClock.getTimeOffset();

    vi.mocked(axios.get).mockRejectedValue(new Error('offline'));
    await asterDEXVirtualClock.syncClientServerTime(true, 'v3');

    expect(asterDEXVirtualClock.getTimeOffset()).toBe(offsetBeforeFailure);
    (global as any).window = originalWindow;
  });

  it('does not install a periodic sync after disposal wins the initial-sync race', async () => {
    vi.useFakeTimers();
    let resolveServerTime: ((response: any) => void) | undefined;
    vi.mocked(axios.get).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveServerTime = resolve;
        }) as any,
    );

    const start = asterDEXVirtualClock.start(true);
    await vi.advanceTimersByTimeAsync(0);
    asterDEXVirtualClock.stop();
    resolveServerTime?.({ data: { serverTime: Date.now() + 1_000 } });
    await start;

    expect(vi.getTimerCount()).toBe(0);
    await vi.advanceTimersByTimeAsync(5 * 60 * 1000);
    expect(axios.get).toHaveBeenCalledTimes(1);
  });
});
