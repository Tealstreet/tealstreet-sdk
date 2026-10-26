import { afterEach, describe, expect, it, vi } from 'vitest';

import { HyperliquidExchange } from '../reference/hyperliquid/hyperliquid.exchange';
import { DefaultStore } from '../../store/store.base';
import type { ExchangeOptions } from '../../types';
import { OrderStopTriggerType } from '../../types';
import { virtualClock } from '../../utils/virtualClock';
import type { AppSettingsWorkerType } from '../../webworker/AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../../webworker/ExchangeMetadataWorkerType';
import { CallMethodHandler } from '../../webworker/handlers/CallMethodHandler';

const NOW = 1_800_000_000_000;
const PRIVATE_KEY = `0x${'11'.repeat(32)}`;
const USER = `0x${'22'.repeat(20)}`;
const VAULT = `0x${'33'.repeat(20)}`;

const createExchange = (overrides: Partial<ExchangeOptions> = {}): HyperliquidExchange => {
  const exchange = new HyperliquidExchange(
    'hyperliquid',
    {
      key: USER,
      secret: PRIVATE_KEY,
      applicationId: USER,
      testnet: true,
      tuning: { consoleLogging: false },
      ...overrides,
    },
    new DefaultStore(),
  );
  exchange.xhr = { post: vi.fn().mockResolvedValue({ data: { status: 'ok' } }) } as any;
  return exchange;
};

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Hyperliquid scheduleCancel dead man switch', () => {
  it('signs and submits an absolute deadline through the exchange endpoint', async () => {
    vi.spyOn(Date, 'now').mockReturnValue(NOW);
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(NOW);
    const exchange = createExchange({ vault: VAULT });
    const deadline = NOW + 30_000;

    await expect(exchange.scheduleCancel(deadline)).resolves.toBeUndefined();

    expect(exchange.xhr.post).toHaveBeenCalledOnce();
    const [path, payload, config] = vi.mocked(exchange.xhr.post).mock.calls[0];
    expect(path).toBe('/exchange');
    expect(payload).toEqual({
      action: { type: 'scheduleCancel', time: deadline },
      nonce: NOW,
      signature: {
        r: expect.stringMatching(/^0x[0-9a-f]{64}$/),
        s: expect.stringMatching(/^0x[0-9a-f]{64}$/),
        v: expect.any(Number),
      },
      vaultAddress: VAULT,
    });
    expect(config).toEqual({ headers: { 'Content-Type': 'application/json' } });
  });

  it.each([null, undefined])('omits time to remove an existing schedule (%s)', async (time) => {
    vi.spyOn(Date, 'now').mockReturnValue(NOW);
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(NOW);
    const exchange = createExchange();

    await exchange.scheduleCancel(time);

    const payload = vi.mocked(exchange.xhr.post).mock.calls[0][1] as Record<string, unknown> & {
      action: Record<string, unknown>;
    };
    expect(payload).toMatchObject({
      action: { type: 'scheduleCancel' },
      nonce: NOW,
    });
    expect(payload.action).not.toHaveProperty('time');
    expect(payload).not.toHaveProperty('vaultAddress');
  });

  it('accepts the documented five-second boundary and rejects unsafe or earlier deadlines before signing', async () => {
    vi.spyOn(Date, 'now').mockReturnValue(NOW);
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(NOW);
    const exchange = createExchange();

    await expect(exchange.scheduleCancel(NOW + 5_000)).resolves.toBeUndefined();
    expect(exchange.xhr.post).toHaveBeenCalledOnce();

    for (const invalid of [NOW + 4_999, Number.NaN, Number.POSITIVE_INFINITY, NOW + 5_000.5]) {
      await expect(exchange.scheduleCancel(invalid)).rejects.toThrow(/safe integer|at least 5 seconds/);
    }
    expect(exchange.xhr.post).toHaveBeenCalledOnce();
  });

  it('propagates transport failures so a caller never mistakes a failed refresh for an armed switch', async () => {
    vi.spyOn(Date, 'now').mockReturnValue(NOW);
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(NOW);
    const exchange = createExchange();
    vi.mocked(exchange.xhr.post).mockRejectedValueOnce(new Error('transport unavailable'));

    await expect(exchange.scheduleCancel(NOW + 30_000)).rejects.toThrow('transport unavailable');
  });

  it('is reachable through the existing typed worker method bridge', async () => {
    vi.spyOn(Date, 'now').mockReturnValue(NOW);
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(NOW);
    const exchange = createExchange();
    const postMessage = vi.fn();
    const handler = new CallMethodHandler(
      'hyperliquid-account',
      'hyperliquid',
      exchange,
      { postMessage } as unknown as Worker,
      {
        placeOrder: {
          tpTriggerType: OrderStopTriggerType.MarkPrice,
          slTriggerType: OrderStopTriggerType.LastPrice,
        },
      } as AppSettingsWorkerType,
      {
        supportedTPSLTriggerTypes: [],
        privateProxyIps: [],
        defaultTPTriggerType: undefined,
        defaultSLTriggerType: undefined,
      } as ExchangeMetadataWorkerType,
    );

    handler.handle({
      op: 'callMethod',
      data: {
        requestId: 'schedule-1',
        method: 'scheduleCancel',
        args: [NOW + 30_000],
      },
    });

    await vi.waitFor(() => {
      expect(exchange.xhr.post).toHaveBeenCalledOnce();
      expect(postMessage).toHaveBeenCalledWith({
        op: 'callMethod:done',
        data: { requestId: 'schedule-1', result: undefined, success: true },
      });
    });
    handler.dispose();
  });
});
