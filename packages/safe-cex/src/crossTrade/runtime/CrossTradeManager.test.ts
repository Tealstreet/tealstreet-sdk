import type { Exchange } from '../../exchanges/base';
import type { Market, Position } from '../../types';
import type { CrossTradeCopier } from '../types';
import type { CrossTradePlaceOrderFn } from './CrossTradeManager';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderSide, PositionSide } from '../../types';
import { loop } from '../../utils/loop';
import { createCrossTradePlaceOrder } from './crossTradePlaceOrder';

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

describe('CrossTradeManager orphan position sizing', () => {
  const auditOrphan = async (positionOverrides: Partial<Position>, marketOverrides: Partial<Market> = {}) => {
    const { CrossTradeManager } = await import('./CrossTradeManager');
    const copier = makeCopier();
    const market = makeMarket({ contractSize: 1000, ...marketOverrides });
    const position: Position = {
      id: 'orphan-position',
      symbol: market.symbol,
      side: PositionSide.Long,
      contracts: 2000,
      notional: 200_000,
      entryPrice: 100,
      leverage: 1,
      unrealizedPnl: 0,
      liquidationPrice: null,
      ...positionOverrides,
    };
    const exchange = (positions: Position[]): Exchange =>
      ({
        name: 'bybit',
        store: {
          loaded: { positions: true },
          positions,
          markets: [market],
          tickers: [{ symbol: market.symbol, last: 100 }],
        },
      }) as unknown as Exchange;
    const masterExchange = exchange([]);
    const copierExchange = exchange([position]);
    const placeOrder = vi.fn().mockResolvedValue({ id: 'close-order' });
    const manager = new CrossTradeManager({
      getExchange: (account) => (account === 'master' ? masterExchange : copierExchange),
      placeOrder,
    });
    manager.start([
      {
        enabled: true,
        masterAccount: 'master',
        masterSymbol: market.symbol,
        masterWeight: 1,
        copiers: [copier],
      },
    ]);
    try {
      expect(loop).toHaveBeenCalledOnce();
      const [tick] = vi.mocked(loop).mock.calls[0];
      await tick();
    } finally {
      manager.stop();
    }
    return { copier, market, placeOrder };
  };

  it.each([1, 3])('refuses an automatic close with conflicting native quantity %s', async (contractsNative) => {
    const { placeOrder } = await auditOrphan({ extra: { contractsNative } });
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it('converts absent native evidence from base to native contracts', async () => {
    const { copier, market, placeOrder } = await auditOrphan({});
    expect(placeOrder).toHaveBeenCalledExactlyOnceWith(copier, market, 100, 'copier1', OrderSide.Sell, 2, true);
  });

  it('derives inverse closes from quote notional when native evidence is absent', async () => {
    const { copier, market, placeOrder } = await auditOrphan(
      { contracts: 2, notional: 600, side: PositionSide.Short },
      { contractSize: 100, isInverse: true },
    );
    expect(placeOrder).toHaveBeenCalledExactlyOnceWith(copier, market, 100, 'copier1', OrderSide.Buy, 6, true);
  });

  it('uses coherent native evidence for the reduce-only orphan close', async () => {
    const { copier, market, placeOrder } = await auditOrphan({ extra: { contractsNative: 2 } });
    expect(placeOrder).toHaveBeenCalledExactlyOnceWith(copier, market, 100, 'copier1', OrderSide.Sell, 2, true);
  });

  it('refuses an inverse close when neither native nor derived quantity is known', async () => {
    const { placeOrder } = await auditOrphan({ contracts: 2, notional: NaN }, { isInverse: true });
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it('refuses invalid present native evidence instead of guessing from base', async () => {
    const { placeOrder } = await auditOrphan({ extra: { contractsNative: NaN } });
    expect(placeOrder).not.toHaveBeenCalled();
  });
});

describe('CrossTradeManager canonical reconciliation', () => {
  const position = (overrides: Partial<Position> = {}): Position => ({
    id: 'position',
    symbol: 'BTCUSDT',
    side: PositionSide.Long,
    contracts: 1000,
    extra: { contractsNative: 1 },
    notional: 100_000,
    entryPrice: 100,
    leverage: 1,
    unrealizedPnl: 0,
    liquidationPrice: null,
    ...overrides,
  });
  const audit = async (
    options: {
      master?: Position[];
      copier?: Position[];
      masterMarket?: Partial<Market>;
      copierMarket?: Partial<Market>;
      masterPrice?: number;
      copierPrice?: number;
      execution?: CrossTradeCopier['execution'];
      weight?: number;
    } = {},
  ) => {
    const { CrossTradeManager } = await import('./CrossTradeManager');
    const masterMarket = makeMarket({
      contractSize: 1000,
      precision: { amount: 0.001, price: 0.1 },
      ...options.masterMarket,
    });
    const copierMarket = makeMarket({
      contractSize: 1000,
      precision: { amount: 0.001, price: 0.1 },
      ...options.copierMarket,
    });
    const createExchange = (positions: Position[], market: Market, price: number) => {
      const placeOrder = vi.fn().mockResolvedValue({ id: 'order' });
      const emit = vi.fn();
      return {
        placeOrder,
        emit,
        exchange: {
          name: 'bybit',
          placeOrder,
          emitter: { emit },
          store: {
            loaded: { positions: true },
            positions,
            markets: [market],
            tickers: [{ symbol: market.symbol, last: price }],
            activeChasers: [],
          },
        } as unknown as Exchange,
      };
    };
    const master = createExchange(options.master ?? [position()], masterMarket, options.masterPrice ?? 100);
    const copier = createExchange(
      options.copier ?? [position({ contracts: 2000, extra: { contractsNative: 2 } })],
      copierMarket,
      options.copierPrice ?? 100,
    );
    const copierConfig = makeCopier({
      execution: options.execution ?? 'Market',
      weight: options.weight ?? 1,
      symbolMappings: { [masterMarket.symbol]: copierMarket.symbol },
    });
    const getExchange = (account: string) => (account === 'master' ? master.exchange : copier.exchange);
    const dispatch = vi.fn(createCrossTradePlaceOrder({ getExchange }));
    const manager = new CrossTradeManager({ getExchange, placeOrder: dispatch });
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    manager.start([
      {
        enabled: true,
        masterAccount: 'master',
        masterSymbol: masterMarket.symbol,
        masterWeight: 1,
        copiers: [copierConfig],
      },
    ]);
    try {
      expect(loop).toHaveBeenCalledOnce();
      const [tick] = vi.mocked(loop).mock.calls[0];
      await tick();
      expect(error).not.toHaveBeenCalled();
    } finally {
      manager.stop();
      error.mockRestore();
    }
    return { dispatch, copier, copierConfig, copierMarket };
  };

  it.each(['Market', 'LimitSlippage', 'Chase'] as const)(
    'refuses the reviewed 1000/1 master, 2000/1 copier case in %s',
    async (execution) => {
      const result = await audit({ copier: [position({ contracts: 2000, extra: { contractsNative: 1 } })], execution });
      expect(result.dispatch).not.toHaveBeenCalled();
      expect(result.copier.placeOrder).not.toHaveBeenCalled();
      expect(result.copier.emit).not.toHaveBeenCalled();
    },
  );

  it('refuses a conflicting master instead of reducing a coherent copier', async () => {
    const result = await audit({ master: [position({ extra: { contractsNative: 2 } })] });
    expect(result.dispatch).not.toHaveBeenCalled();
  });

  it('does not treat an unknown master quantity as an absent master position', async () => {
    const result = await audit({ master: [position({ contracts: NaN, extra: undefined })] });
    expect(result.dispatch).not.toHaveBeenCalled();
  });

  it('does not treat an unknown copier quantity as an absent copier position', async () => {
    const result = await audit({ copier: [position({ contracts: NaN, extra: undefined })] });
    expect(result.dispatch).not.toHaveBeenCalled();
  });

  it.each(['Market', 'LimitSlippage', 'Chase'] as const)(
    'blocks an offsetting entry after an unknown opposite-side orphan close in %s',
    async (execution) => {
      const result = await audit({
        copier: [position({ side: PositionSide.Short, contracts: 2000, extra: { contractsNative: 1 } })],
        execution,
      });
      expect(result.dispatch).not.toHaveBeenCalled();
      expect(result.copier.placeOrder).not.toHaveBeenCalled();
      expect(result.copier.emit).not.toHaveBeenCalled();
    },
  );

  it('does not open against a zero-base row with invalid native evidence', async () => {
    const result = await audit({ copier: [position({ contracts: 0, extra: { contractsNative: NaN } })] });
    expect(result.dispatch).not.toHaveBeenCalled();
  });

  it.each(['Market', 'LimitSlippage', 'Chase'] as const)(
    'reduces the coherent 1000/1 master, 2000/2 copier by one in %s',
    async (execution) => {
      const result = await audit({ execution });
      expect(result.dispatch).toHaveBeenCalledExactlyOnceWith(
        result.copierConfig,
        result.copierMarket,
        100,
        'copier1',
        OrderSide.Sell,
        1,
        true,
      );
      if (execution === 'Chase') {
        expect(result.copier.emit).toHaveBeenCalledExactlyOnceWith(
          'chaser:start',
          expect.objectContaining({ size: 1, side: OrderSide.Sell, reduceOnly: true }),
        );
      } else {
        expect(result.copier.placeOrder).toHaveBeenCalledExactlyOnceWith(
          expect.objectContaining({ amount: 1, side: OrderSide.Sell, reduceOnly: true }),
        );
      }
    },
  );

  it('derives both linear sizes when native evidence is absent', async () => {
    const result = await audit({
      master: [position({ extra: undefined })],
      copier: [position({ contracts: 2000, extra: undefined })],
    });
    expect(result.dispatch).toHaveBeenCalledExactlyOnceWith(
      result.copierConfig,
      result.copierMarket,
      100,
      'copier1',
      OrderSide.Sell,
      1,
      true,
    );
  });

  it('uses known native-only copier quantity instead of its unusable base field', async () => {
    const result = await audit({ copier: [position({ contracts: NaN, extra: { contractsNative: 2 } })] });
    expect(result.dispatch).toHaveBeenCalledExactlyOnceWith(
      result.copierConfig,
      result.copierMarket,
      100,
      'copier1',
      OrderSide.Sell,
      1,
      true,
    );
  });

  it('uses canonical inverse notionals instead of unrelated base figures', async () => {
    const result = await audit({
      masterMarket: { isInverse: true, contractSize: 100 },
      copierMarket: { isInverse: true, contractSize: 100 },
      master: [position({ contracts: 99, notional: 100, extra: { contractsNative: 1 } })],
      copier: [position({ contracts: 88, notional: 200, extra: { contractsNative: 2 } })],
    });
    expect(result.dispatch).toHaveBeenCalledExactlyOnceWith(
      result.copierConfig,
      result.copierMarket,
      100,
      'copier1',
      OrderSide.Sell,
      1,
      true,
    );
  });

  it('converts the canonical copier size for a linear-master/inverse-copier reduction', async () => {
    const result = await audit({
      copierMarket: { isInverse: true, contractSize: 100 },
      copier: [position({ contracts: 2, notional: 200_000, extra: { contractsNative: 2000 } })],
    });
    expect(result.dispatch).toHaveBeenCalledExactlyOnceWith(
      result.copierConfig,
      result.copierMarket,
      100,
      'copier1',
      OrderSide.Sell,
      1000,
      true,
    );
  });

  it('uses a canonical inverse master even when its base figure is zero', async () => {
    const result = await audit({
      masterMarket: { isInverse: true, contractSize: 100 },
      copierMarket: { isInverse: true, contractSize: 100 },
      master: [position({ contracts: 0, notional: 100, extra: { contractsNative: 1 } })],
      copier: [],
    });
    expect(result.dispatch).toHaveBeenCalledExactlyOnceWith(
      result.copierConfig,
      result.copierMarket,
      100,
      'copier1',
      OrderSide.Buy,
      1,
      false,
    );
  });

  it('opens from a known flat copier row', async () => {
    const result = await audit({ copier: [position({ contracts: 0, extra: { contractsNative: 0 } })] });
    expect(result.dispatch).toHaveBeenCalledExactlyOnceWith(
      result.copierConfig,
      result.copierMarket,
      100,
      'copier1',
      OrderSide.Buy,
      1,
      false,
    );
  });

  it('caps a lot-rounded reduction at the known canonical copier quantity', async () => {
    const result = await audit({
      masterMarket: { contractSize: 1 },
      copierMarket: { contractSize: 1 },
      master: [position({ contracts: 0.00001, extra: { contractsNative: 0.00001 } })],
      copier: [position({ contracts: 0.0006, extra: { contractsNative: 0.0006 } })],
    });
    expect(result.dispatch).toHaveBeenCalledExactlyOnceWith(
      result.copierConfig,
      result.copierMarket,
      100,
      'copier1',
      OrderSide.Sell,
      0.0006,
      true,
    );
  });

  it('preserves relative weights for matched reductions', async () => {
    const result = await audit({ weight: 0.5 });
    expect(result.dispatch).toHaveBeenCalledExactlyOnceWith(
      result.copierConfig,
      result.copierMarket,
      100,
      'copier1',
      OrderSide.Sell,
      1.5,
      true,
    );
  });

  it('preserves the close side for an oppositely weighted copier', async () => {
    const result = await audit({
      weight: -1,
      copier: [position({ side: PositionSide.Short, contracts: 2000, extra: { contractsNative: 2 } })],
    });
    expect(result.dispatch).toHaveBeenCalledExactlyOnceWith(
      result.copierConfig,
      result.copierMarket,
      100,
      'copier1',
      OrderSide.Buy,
      1,
      true,
    );
  });

  it('preserves quote conversion between different bases and contract sizes', async () => {
    const result = await audit({
      masterPrice: 200,
      copierMarket: { id: 'ETHUSDT', symbol: 'ETHUSDT', base: 'ETH', contractSize: 100 },
      copier: [position({ symbol: 'ETHUSDT', contracts: 3000, extra: { contractsNative: 30 } })],
    });
    expect(result.dispatch).toHaveBeenCalledExactlyOnceWith(
      result.copierConfig,
      result.copierMarket,
      100,
      'copier1',
      OrderSide.Sell,
      10,
      true,
    );
  });
});
