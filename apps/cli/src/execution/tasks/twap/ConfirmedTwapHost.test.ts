import { afterEach, describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '@tealstreet/safe-cex/src/exchanges/bybit/bybit.exchange';
import type { Market } from '@tealstreet/safe-cex/src/types';
import { OrderSide } from '@tealstreet/safe-cex/src/types';

import type { ExactQuantity } from '../../confirmed';
import { createConfirmedTwapHost } from './ConfirmedTwapHost';
import type { MarketTwapSpec } from './MarketTwapTask';
import { formatConfirmedFinalReport, oneShotExitCode } from './confirmedControls';

const market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.01, price: 0.1 },
  limits: { amount: { min: 0.01, max: 100 }, leverage: { min: 1, max: 100 } },
} as Market;

const store = {
  subscribe: vi.fn(),
  markets: [market],
  marketsBySymbol: { BTCUSDT: market },
  tickersBySymbol: {},
  orders: [],
  positions: [],
} as never;

const quantity = (decimal: string): ExactQuantity => {
  const [, fraction = ''] = decimal.split('.');
  return { decimal, atoms: decimal.replace('.', ''), atomScale: fraction.length };
};

const spec = (taskId: string, overrides: Partial<MarketTwapSpec> = {}): MarketTwapSpec => ({
  taskId,
  market,
  side: OrderSide.Buy,
  target: quantity('0.02'),
  quantityStep: quantity('0.01'),
  minQuantity: quantity('0.01'),
  maxQuantity: quantity('100'),
  contractSize: quantity('1'),
  sliceCount: 2,
  startAtMs: 1_000,
  durationMs: 1_000,
  ...overrides,
});

const filledLookup = (wireClientOrderId: string, amount = '0.01') => ({
  retCode: 0,
  result: {
    category: 'linear',
    list: [
      {
        orderLinkId: wireClientOrderId,
        orderId: 'venue-1',
        symbol: market.symbol,
        orderStatus: 'Filled',
        qty: amount,
        cumExecQty: amount,
        avgPrice: '100',
      },
    ],
  },
});

describe('ConfirmedTwapHost', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('continues later slices while an earlier exact lookup is indeterminate', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, store);
    exchange.placeOrder = vi.fn().mockResolvedValueOnce(['venue-1']).mockResolvedValueOnce(['venue-2']);
    exchange.getOrderByClientOrderId = vi.fn().mockResolvedValue({
      retCode: 0,
      result: { category: 'linear', list: [] },
    });
    const host = createConfirmedTwapHost({
      accountName: 'bybit-main',
      exchange,
      signerFingerprint: 'signer',
    });

    await host.start(spec('twap-1'));
    await vi.advanceTimersByTimeAsync(1_200);

    expect(exchange.placeOrder).toHaveBeenCalledTimes(2);
    expect(host.list()[0]).toMatchObject({
      lifecycle: 'running',
      reservedUnresolved: { atoms: '2' },
    });
    expect(host.hasLiveWork()).toBe(true);
  });

  it('settles its handle after every slice is exact-ID confirmed filled', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, store);
    exchange.placeOrder = vi.fn().mockResolvedValue(['venue-1']);
    exchange.getOrderByClientOrderId = vi
      .fn()
      .mockImplementation((_symbol, wireClientOrderId) => Promise.resolve(filledLookup(wireClientOrderId)));
    const onTaskSettled = vi.fn();
    const host = createConfirmedTwapHost({
      accountName: 'bybit-main',
      exchange,
      signerFingerprint: 'signer',
      onTaskSettled,
    });

    const handle = await host.start(spec('twap-complete', { target: quantity('0.01'), sliceCount: 1, durationMs: 0 }));
    expect(handle.outcomeDeadlineAtMs).toBe(16_000);
    await vi.advanceTimersByTimeAsync(200);

    await expect(handle.settled).resolves.toMatchObject({
      lifecycle: 'completed',
      confirmedFilled: { atoms: '1' },
      reservedUnresolved: { atoms: '0' },
      residual: { atoms: '0' },
    });
    expect(host.hasLiveWork()).toBe(false);
    expect(onTaskSettled).toHaveBeenCalledWith(expect.objectContaining({ taskId: 'twap-complete' }));
    expect(host.list()).toEqual([]);
  });

  it('retains its task and settled handle while stopAll fences an in-flight placement', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const placement = Promise.withResolvers<string[]>();
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, store);
    exchange.placeOrder = vi.fn().mockReturnValue(placement.promise);
    exchange.getOrderByClientOrderId = vi
      .fn()
      .mockImplementation((_symbol, wireClientOrderId) => Promise.resolve(filledLookup(wireClientOrderId)));
    const onTaskSettled = vi.fn();
    const host = createConfirmedTwapHost({
      accountName: 'bybit-main',
      exchange,
      signerFingerprint: 'signer',
      onTaskSettled,
    });

    const handle = await host.start(spec('twap-stop', { target: quantity('0.01'), sliceCount: 1, durationMs: 0 }));
    await vi.advanceTimersByTimeAsync(0);
    const stopping = host.stopAll();
    let stopResolved = false;
    let handleResolved = false;
    void stopping.then(() => {
      stopResolved = true;
    });
    void handle.settled.then(() => {
      handleResolved = true;
    });
    await Promise.resolve();

    expect(stopResolved).toBe(false);
    expect(handleResolved).toBe(false);
    expect(host.list()).toHaveLength(1);
    expect(host.hasLiveWork()).toBe(true);
    expect(onTaskSettled).not.toHaveBeenCalled();

    placement.resolve(['venue-1']);
    await vi.advanceTimersByTimeAsync(200);

    await expect(stopping).resolves.toMatchObject([
      {
        lifecycle: 'stopped',
        confirmedFilled: { atoms: '1' },
        reservedUnresolved: { atoms: '0' },
      },
    ]);
    await expect(handle.settled).resolves.toMatchObject({ taskId: 'twap-stop' });
    expect(onTaskSettled).toHaveBeenCalledWith(expect.objectContaining({ taskId: 'twap-stop' }));
    expect(host.list()).toEqual([]);
  });
  it('publishes an incident outcome for one-shot exit without evicting the interactive task', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, store);
    exchange.placeOrder = vi.fn().mockResolvedValue(['venue-incident']);
    exchange.getOrderByClientOrderId = vi
      .fn()
      .mockImplementation((_symbol, wireClientOrderId) => Promise.resolve(filledLookup(wireClientOrderId, '0.02')));
    const host = createConfirmedTwapHost({
      accountName: 'bybit-main',
      exchange,
      signerFingerprint: 'signer',
    });

    const handle = await host.start(spec('twap-incident', { target: quantity('0.01'), sliceCount: 1, durationMs: 0 }));
    let settled = false;
    void handle.settled.then(() => {
      settled = true;
    });
    await vi.advanceTimersByTimeAsync(200);

    const outcome = await handle.outcome;
    expect(outcome).toMatchObject({
      lifecycle: 'paused',
      reservedUnresolved: { atoms: '1' },
      slices: [{ truth: { kind: 'incident' } }],
    });
    expect(formatConfirmedFinalReport(outcome).join('\n')).toContain('status=incident:');
    expect(oneShotExitCode([outcome])).toBe(3);
    expect(settled).toBe(false);
    expect(host.list()).toHaveLength(1);
    expect(host.hasLiveWork()).toBe(true);

    const stopping = host.stopAll();
    await vi.advanceTimersByTimeAsync(15_000);
    await expect(stopping).resolves.toMatchObject([
      {
        lifecycle: 'stopping',
        slices: [{ truth: { kind: 'incident' } }],
      },
    ]);
    expect(settled).toBe(false);
    expect(host.list()).toHaveLength(1);
    expect(host.hasLiveWork()).toBe(true);
  });
});
