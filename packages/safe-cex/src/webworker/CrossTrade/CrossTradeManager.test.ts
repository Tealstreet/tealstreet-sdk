import type { CrossTradeCopier } from '../../crossTrade/types';
import type { Market } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderSide } from '../../types';

vi.mock('../../utils/loop', () => ({
  loop: vi.fn(),
}));

const exchangeWorkersMock: Record<string, any> = {};
vi.mock('../store.webworker', () => ({
  exchangeWorkers: exchangeWorkersMock,
}));

const crossTradePlaceOrderMock = vi.fn();
vi.mock('./crossTradePlaceOrder', () => ({
  crossTradePlaceOrder: crossTradePlaceOrderMock,
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

  beforeEach(async () => {
    const { CrossTradeManager } = await import('./CrossTradeManager');
    manager = new CrossTradeManager();
    crossTradePlaceOrderMock.mockReset();
    crossTradePlaceOrderMock.mockResolvedValue({ id: 'order-1' });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('places an order under normal conditions', async () => {
    const market = makeMarket();
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(crossTradePlaceOrderMock).toHaveBeenCalledTimes(1);
  });

  it('clears state on success so future calls go through', async () => {
    const market = makeMarket();
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(crossTradePlaceOrderMock).toHaveBeenCalledTimes(2);
  });

  it('skips orders below market.limits.amount.min', async () => {
    const market = makeMarket({
      limits: { amount: { min: 1, max: 1000 }, leverage: { min: 1, max: 100 } },
    });
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(crossTradePlaceOrderMock).not.toHaveBeenCalled();
  });

  it('allows orders at exactly the min amount threshold', async () => {
    const market = makeMarket({
      limits: { amount: { min: 1, max: 1000 }, leverage: { min: 1, max: 100 } },
    });
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 1, false);
    expect(crossTradePlaceOrderMock).toHaveBeenCalledTimes(1);
  });

  it('does not impose a min when market.limits.amount.min is 0', async () => {
    const market = makeMarket({
      limits: { amount: { min: 0, max: 1000 }, leverage: { min: 1, max: 100 } },
    });
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.0001, false);
    expect(crossTradePlaceOrderMock).toHaveBeenCalledTimes(1);
  });

  it('reduceOnly orders bypass the min-amount gate so residual closes still flow', async () => {
    const market = makeMarket({
      limits: { amount: { min: 1, max: 1000 }, leverage: { min: 1, max: 100 } },
    });
    // Below-min, but reduceOnly = true → must reach the exchange
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Sell, 0.5, true);
    expect(crossTradePlaceOrderMock).toHaveBeenCalledTimes(1);
    expect(crossTradePlaceOrderMock.mock.calls[0][6]).toBe(true);
  });

  it('mutes (account, symbol) after 2 consecutive failures', async () => {
    const market = makeMarket();
    crossTradePlaceOrderMock.mockRejectedValue(new Error('boom'));

    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(crossTradePlaceOrderMock).toHaveBeenCalledTimes(2);

    // 3rd call should be muted, never reach crossTradePlaceOrder
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(crossTradePlaceOrderMock).toHaveBeenCalledTimes(2);
  });

  it('mute is scoped per (account, symbol)', async () => {
    const btcMarket = makeMarket({ id: 'BTCUSDT', symbol: 'BTCUSDT' });
    const ethMarket = makeMarket({ id: 'ETHUSDT', symbol: 'ETHUSDT' });
    crossTradePlaceOrderMock.mockRejectedValue(new Error('boom'));

    await manager._safePlaceOrder(makeCopier(), btcMarket, 100, 'copier1', OrderSide.Buy, 0.5, false);
    await manager._safePlaceOrder(makeCopier(), btcMarket, 100, 'copier1', OrderSide.Buy, 0.5, false);
    crossTradePlaceOrderMock.mockClear();

    // BTC on copier1 muted; ETH on copier1 should still be attempted
    await manager._safePlaceOrder(makeCopier(), ethMarket, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(crossTradePlaceOrderMock).toHaveBeenCalledTimes(1);

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
    expect(crossTradePlaceOrderMock).toHaveBeenCalledTimes(2);
  });

  it('mute expires after 30s', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-01-01T00:00:00Z'));
    const market = makeMarket();
    crossTradePlaceOrderMock.mockRejectedValue(new Error('boom'));

    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    crossTradePlaceOrderMock.mockClear();
    crossTradePlaceOrderMock.mockResolvedValue({ id: 'order-1' });

    // Still muted at +29s
    vi.setSystemTime(new Date('2026-01-01T00:00:29Z'));
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(crossTradePlaceOrderMock).not.toHaveBeenCalled();

    // Unmuted at +31s
    vi.setSystemTime(new Date('2026-01-01T00:00:31Z'));
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(crossTradePlaceOrderMock).toHaveBeenCalledTimes(1);
  });

  it('successful call clears prior failure count', async () => {
    const market = makeMarket();
    crossTradePlaceOrderMock.mockRejectedValueOnce(new Error('boom'));
    crossTradePlaceOrderMock.mockResolvedValueOnce({ id: 'order-1' });
    crossTradePlaceOrderMock.mockRejectedValueOnce(new Error('boom2'));

    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    // After success, the failure count was cleared. One more failure should NOT yet mute.
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    crossTradePlaceOrderMock.mockClear();
    crossTradePlaceOrderMock.mockResolvedValue({ id: 'order-2' });

    // Not muted — one isolated failure is below threshold
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(crossTradePlaceOrderMock).toHaveBeenCalledTimes(1);
  });

  it('stop() clears failure state', async () => {
    const market = makeMarket();
    crossTradePlaceOrderMock.mockRejectedValue(new Error('boom'));

    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);

    manager.stop();

    crossTradePlaceOrderMock.mockClear();
    crossTradePlaceOrderMock.mockResolvedValue({ id: 'order-1' });

    // After stop(), the mute is cleared; next call goes through
    await manager._safePlaceOrder(makeCopier(), market, 100, 'copier1', OrderSide.Buy, 0.5, false);
    expect(crossTradePlaceOrderMock).toHaveBeenCalledTimes(1);
  });
});

describe('CrossTradeManager.start / stop lifecycle', () => {
  let manager: any;

  beforeEach(async () => {
    const { CrossTradeManager } = await import('./CrossTradeManager');
    manager = new CrossTradeManager();
    crossTradePlaceOrderMock.mockReset();
    crossTradePlaceOrderMock.mockResolvedValue({ id: 'order-1' });

    for (const key of Object.keys(exchangeWorkersMock)) delete exchangeWorkersMock[key];
  });

  it('start([]) schedules a loop but places no orders', async () => {
    manager.start([]);
    await new Promise((r) => setTimeout(r, 0));
    expect(crossTradePlaceOrderMock).not.toHaveBeenCalled();
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
