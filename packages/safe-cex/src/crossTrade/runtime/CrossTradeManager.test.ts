import type { Market } from '../../types';
import type { CrossTradeCopier } from '../types';
import type { CrossTradePlaceOrderFn } from './CrossTradeManager';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderSide } from '../../types';

vi.mock('../../utils/loop', () => ({
  loop: vi.fn(),
}));

const makeMarket = (overrides: Partial<Market> = {}): Market => ({
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 3, price: 1 },
  limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 100 } },
  ...overrides,
});

const makeCopier = (overrides: Partial<CrossTradeCopier> = {}): CrossTradeCopier => ({
  enabled: true,
  account: 'copier1',
  weight: 1,
  execution: 'Market',
  weightMode: 'Absolute',
  symbolMappings: {},
  ...overrides,
});

describe('CrossTradeManager._safePlaceOrder', () => {
  let manager: any;
  let placeOrderMock: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    const { CrossTradeManager } = await import('./CrossTradeManager');
    placeOrderMock = vi.fn().mockResolvedValue({ id: 'order-1' });
    manager = new CrossTradeManager({
      getExchange: () => undefined,
      placeOrder: placeOrderMock as unknown as CrossTradePlaceOrderFn,
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('places an order under normal conditions', async () => {
    const market = makeMarket();
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(placeOrderMock).toHaveBeenCalledTimes(1);
  });

  it('clears state on success so future calls go through', async () => {
    const market = makeMarket();
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(placeOrderMock).toHaveBeenCalledTimes(2);
  });

  it('skips orders below market.limits.amount.min', async () => {
    const market = makeMarket({
      limits: { amount: { min: 1, max: 1000 }, leverage: { min: 1, max: 100 } },
    });
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(placeOrderMock).not.toHaveBeenCalled();
  });

  it('allows orders at exactly the min amount threshold', async () => {
    const market = makeMarket({
      limits: { amount: { min: 1, max: 1000 }, leverage: { min: 1, max: 100 } },
    });
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 1, false);
    expect(placeOrderMock).toHaveBeenCalledTimes(1);
  });

  it('does not impose a min when market.limits.amount.min is 0', async () => {
    const market = makeMarket({
      limits: { amount: { min: 0, max: 1000 }, leverage: { min: 1, max: 100 } },
    });
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.0001, false);
    expect(placeOrderMock).toHaveBeenCalledTimes(1);
  });

  it('reduceOnly orders bypass the min-amount gate so residual closes still flow', async () => {
    const market = makeMarket({
      limits: { amount: { min: 1, max: 1000 }, leverage: { min: 1, max: 100 } },
    });
    // Below-min, but reduceOnly = true → must reach the injected placeOrder
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Sell, 0.5, true);
    expect(placeOrderMock).toHaveBeenCalledTimes(1);
    expect(placeOrderMock.mock.calls[0][6]).toBe(true);
  });

  it('mutes (account, symbol) after 2 consecutive failures', async () => {
    const market = makeMarket();
    placeOrderMock.mockReset();
    placeOrderMock.mockRejectedValue(new Error('boom'));

    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(placeOrderMock).toHaveBeenCalledTimes(2);

    // 3rd call should be muted, never reach placeOrder
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(placeOrderMock).toHaveBeenCalledTimes(2);
  });

  it('mute is scoped per (account, symbol)', async () => {
    const btcMarket = makeMarket({ id: 'BTCUSDT', symbol: 'BTCUSDT' });
    const ethMarket = makeMarket({ id: 'ETHUSDT', symbol: 'ETHUSDT' });
    placeOrderMock.mockReset();
    placeOrderMock.mockRejectedValue(new Error('boom'));

    await manager._safePlaceOrder(makeCopier(), btcMarket, 100, 'copier1', OrderSide.Buy, 0.5, false);
    await manager._safePlaceOrder(makeCopier(), btcMarket, 100, 'copier1', OrderSide.Buy, 0.5, false);
    placeOrderMock.mockClear();

    // BTC on copier1 muted; ETH on copier1 should still be attempted
    await manager._safePlaceOrder(makeCopier(), ethMarket, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(placeOrderMock).toHaveBeenCalledTimes(1);

    // BTC on copier2 should also still be attempted
    await manager._safePlaceOrder(
      makeCopier({ account: 'copier2' }),
      btcMarket,
      100,
      'copier2',
      OrderSide.Buy,
      0.5,
      false,
    );
    expect(placeOrderMock).toHaveBeenCalledTimes(2);
  });

  // Bravulasco: an uncertain refusal was retried every 3 s loop. It is a hold, not a failure.
  it('holds a symbol refused as uncertain for 15 s, doubling to 5 min, until a placement lands', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-01-01T00:00:00Z'));
    const market = makeMarket();
    const uncertain = new Error('Placement outcome is uncertain; reconcile before placing or amending');
    placeOrderMock.mockReset();
    placeOrderMock.mockRejectedValue(uncertain);
    const attempt = () => manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    const attemptsAfter = async (ms: number) => {
      placeOrderMock.mockClear();
      vi.advanceTimersByTime(ms);
      await attempt();
      return placeOrderMock.mock.calls.length;
    };
    await attempt();
    expect(placeOrderMock).toHaveBeenCalledOnce();
    // Every 3 s loop inside the hold sends nothing.
    expect(await attemptsAfter(3_000)).toBe(0);
    expect(await attemptsAfter(11_999)).toBe(0);
    expect(await attemptsAfter(1)).toBe(1); // 15 s
    expect(await attemptsAfter(29_999)).toBe(0);
    expect(await attemptsAfter(1)).toBe(1); // 30 s
    expect(await attemptsAfter(60_000)).toBe(1); // 60 s
    expect(await attemptsAfter(120_000)).toBe(1); // 120 s
    expect(await attemptsAfter(240_000)).toBe(1); // 240 s
    expect(await attemptsAfter(299_999)).toBe(0); // capped at 5 min
    expect(await attemptsAfter(1)).toBe(1);
    // A placement that lands ends the hold.
    placeOrderMock.mockResolvedValue({ id: 'order-1' });
    expect(await attemptsAfter(300_000)).toBe(1);
    expect(await attemptsAfter(3_000)).toBe(1);
  });

  it('logs each placement attempt as Cross Trade, with venue and symbol but no account name', async () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => {});
    const market = makeMarket();
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(info).toHaveBeenCalledWith(
      '[CrossTrade] placing',
      expect.objectContaining({ source: 'cross-trade', symbol: 'BTCUSDT', side: OrderSide.Buy, reduceOnly: false }),
    );
    expect(JSON.stringify(info.mock.calls)).not.toContain('copier1');
    info.mockRestore();
  });

  it('mute expires after 30s', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-01-01T00:00:00Z'));
    const market = makeMarket();
    placeOrderMock.mockReset();
    placeOrderMock.mockRejectedValue(new Error('boom'));

    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    placeOrderMock.mockClear();
    placeOrderMock.mockResolvedValue({ id: 'order-1' });

    // Still muted at +29s
    vi.setSystemTime(new Date('2026-01-01T00:00:29Z'));
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(placeOrderMock).not.toHaveBeenCalled();

    // Unmuted at +31s
    vi.setSystemTime(new Date('2026-01-01T00:00:31Z'));
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(placeOrderMock).toHaveBeenCalledTimes(1);
  });

  it('successful call clears prior failure count', async () => {
    const market = makeMarket();
    placeOrderMock.mockReset();
    placeOrderMock.mockRejectedValueOnce(new Error('boom'));
    placeOrderMock.mockResolvedValueOnce({ id: 'order-1' });
    placeOrderMock.mockRejectedValueOnce(new Error('boom2'));

    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    // After success, the failure count was cleared. One more failure should NOT yet mute.
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    placeOrderMock.mockClear();
    placeOrderMock.mockResolvedValue({ id: 'order-2' });

    // Not muted — one isolated failure is below threshold
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(placeOrderMock).toHaveBeenCalledTimes(1);
  });

  it('stop() clears failure state', async () => {
    const market = makeMarket();
    placeOrderMock.mockReset();
    placeOrderMock.mockRejectedValue(new Error('boom'));

    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);

    manager.stop();

    placeOrderMock.mockClear();
    placeOrderMock.mockResolvedValue({ id: 'order-1' });

    // After stop(), the mute is cleared; next call goes through
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(placeOrderMock).toHaveBeenCalledTimes(1);
  });
});

describe('CrossTradeManager.start / stop lifecycle', () => {
  let manager: any;

  beforeEach(async () => {
    const { CrossTradeManager } = await import('./CrossTradeManager');
    manager = new CrossTradeManager({
      getExchange: () => undefined,
      placeOrder: vi.fn().mockResolvedValue({ id: 'order-1' }) as unknown as CrossTradePlaceOrderFn,
    });
  });

  it('start([]) schedules a loop but places no orders', async () => {
    manager.start([]);
    await new Promise((r) => setTimeout(r, 0));
    expect(manager._loop).toBeDefined();
    manager.stop();
  });

  it('stop() marks the current loop disposed', async () => {
    manager.start([]);
    const loopBeforeStop = manager._loop;
    manager.stop();
    expect(loopBeforeStop.isDisposed).toBe(true);
  });

  it('calling start() twice disposes the previous loop', async () => {
    manager.start([]);
    const firstLoop = manager._loop;
    manager.start([]);
    expect(firstLoop.isDisposed).toBe(true);
    expect(manager._loop).not.toBe(firstLoop);
    manager.stop();
  });
});
