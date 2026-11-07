import { describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '../reference/bitget/bitget.exchange';
import { ENDPOINTS } from '../../exchanges/bitget/bitget.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderType, PositionMode, PositionSide } from '../../types';

const market = {
  id: 'TRXUSDT',
  symbol: 'TRXUSDT',
  base: 'TRX',
  quote: 'USDT',
  active: true,
  precision: { amount: 1, price: 0.00001 },
  limits: { amount: { min: 1, max: 1000000 }, leverage: { min: 1, max: 100 } },
};

const openPosition = {
  id: 'TRXUSDT:long',
  symbol: 'TRXUSDT',
  side: PositionSide.Long,
  entryPrice: 0.34,
  notional: 34,
  leverage: 10,
  unrealizedPnl: 0,
  realizedPnl: 0,
  contracts: 100,
  liquidationPrice: 0,
  positionMode: PositionMode.OneWay,
  marginType: MarginType.Cross,
} as any;

const buildExchange = () => {
  const exchange = new BitgetExchange('bitget', { key: 'k', secret: 's', passphrase: 'p' }, new DefaultStore());
  exchange.store.update({ markets: [market] as any });
  // Real batch placement lazily detects the account type. Keep these Classic
  // fixtures off the network while exercising that detection path.
  vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: { code: '40084', msg: 'Classic Account mode' } });
  vi.spyOn(exchange as any, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
  return exchange;
};

const sizedStop = {
  symbol: 'TRXUSDT',
  type: OrderType.StopLoss,
  side: OrderSide.Sell,
  amount: 30,
  price: 0.33509,
  reduceOnly: true,
};

describe('audit regression: Bitget sized stop plan type', () => {
  it('keeps a sized stop sized when the position is only reachable over REST', async () => {
    // The hosted-mode mirror executes order writes without ever start()ing, so
    // store.positions is empty. This used to downgrade the stop to pos_loss and
    // drop `size` entirely, so the UI rendered it as "Full Position".
    const exchange = buildExchange();
    const fetchOpenPositions = vi.fn().mockResolvedValue([openPosition]);
    exchange.fetchOpenPositions = fetchOpenPositions;

    const payload = await exchange.formatAlgoOrder(sizedStop as any);

    expect(payload.planType).toBe('loss_plan');
    expect(payload.size).toBe('30');
    expect(fetchOpenPositions).toHaveBeenCalledTimes(1);
  });

  it('caches the REST snapshot across stops instead of refetching per order', async () => {
    const exchange = buildExchange();
    const fetchOpenPositions = vi.fn().mockResolvedValue([openPosition]);
    exchange.fetchOpenPositions = fetchOpenPositions;

    await Promise.all([
      exchange.formatAlgoOrder(sizedStop as any),
      exchange.formatAlgoOrder({ ...sizedStop, type: OrderType.TakeProfit, price: 0.4 } as any),
    ]);

    expect(fetchOpenPositions).toHaveBeenCalledTimes(1);
  });

  it('still fetches when the store holds only other symbols', async () => {
    // The worker streams positions into the mirror, so a store carrying an
    // unrelated symbol looks loaded while the one being stopped is absent.
    const exchange = buildExchange();
    exchange.store.update({
      positions: [{ ...openPosition, id: 'BTCUSDT:long', symbol: 'BTCUSDT' }] as any,
    });
    const fetchOpenPositions = vi.fn().mockResolvedValue([openPosition]);
    exchange.fetchOpenPositions = fetchOpenPositions;

    const payload = await exchange.formatAlgoOrder(sizedStop as any);

    expect(fetchOpenPositions).toHaveBeenCalledTimes(1);
    expect(payload.planType).toBe('loss_plan');
    expect(payload.size).toBe('30');
  });

  it('sizes a stop protecting a short when the store only holds a stale long', async () => {
    // Observed live: 4 buy-side stops went out as pos_loss while sell-side ones
    // on the same symbol were correctly sized. A symbol-only store check looked
    // satisfied by the leftover long row, so the short that would justify the
    // sized stop was never fetched.
    const exchange = buildExchange();
    exchange.store.update({ positions: [{ ...openPosition, contracts: 0 }] as any });
    const shortPosition = {
      ...openPosition,
      id: 'TRXUSDT:short',
      side: PositionSide.Short,
      contracts: 100,
    };
    const fetchOpenPositions = vi.fn().mockResolvedValue([shortPosition]);
    exchange.fetchOpenPositions = fetchOpenPositions;

    // Buy-side stop => protects a SHORT position.
    const payload = await exchange.formatAlgoOrder({ ...sizedStop, side: OrderSide.Buy } as any);

    expect(fetchOpenPositions).toHaveBeenCalledTimes(1);
    expect(payload.planType).toBe('loss_plan');
    expect(payload.size).toBe('30');
  });

  it('skips the fetch when the store already answers for that exact side', async () => {
    const exchange = buildExchange();
    exchange.store.update({ positions: [openPosition] as any });
    const fetchOpenPositions = vi.fn().mockResolvedValue([]);
    exchange.fetchOpenPositions = fetchOpenPositions;

    const payload = await exchange.formatAlgoOrder(sizedStop as any);

    expect(fetchOpenPositions).not.toHaveBeenCalled();
    expect(payload.planType).toBe('loss_plan');
  });

  it('keeps a whole-position stop sized when the request overshoots by less than a lot', async () => {
    // A "close everything" stop derived from a percentage lands at 30.34 on a
    // 30-contract position. The size actually POSTed is adjust(30.34, 1) = 30,
    // which fits — but comparing the RAW amount against contracts failed and
    // downgraded it to pos_loss. Observed live as 4 such stops in one run.
    const exchange = buildExchange();
    exchange.fetchOpenPositions = vi.fn().mockResolvedValue([{ ...openPosition, contracts: 30 }]);

    const payload = await exchange.formatAlgoOrder({ ...sizedStop, amount: 30.341646 } as any);

    expect(payload.planType).toBe('loss_plan');
    expect(payload.size).toBe('30');
  });

  it('clamps to the position instead of downgrading when the sent size exceeds it', async () => {
    // Bitget rejects an oversized loss_plan, but silently converting a stop the
    // user deliberately sized into a whole-position one is the worse failure.
    const exchange = buildExchange();
    exchange.fetchOpenPositions = vi.fn().mockResolvedValue([{ ...openPosition, contracts: 30 }]);

    const payload = await exchange.formatAlgoOrder({ ...sizedStop, amount: 45 } as any);

    expect(payload.planType).toBe('loss_plan');
    expect(payload.size).toBe('30');
  });

  it('clamps a one-lot overshoot rather than dropping the size', async () => {
    // The live failure: price drift between opening the position and sizing the
    // stop asked for 31 against a 30-contract long, and the whole stop became a
    // full-position one (e2e run 31353474704).
    const exchange = buildExchange();
    exchange.fetchOpenPositions = vi.fn().mockResolvedValue([{ ...openPosition, contracts: 30 }]);

    const payload = await exchange.formatAlgoOrder({ ...sizedStop, amount: 31 } as any);

    expect(payload.planType).toBe('loss_plan');
    expect(payload.size).toBe('30');
  });

  it('floors the clamp to the lot so it cannot round back above the position', async () => {
    // `adjust` rounds to NEAREST, and position contracts come straight off the
    // venue unaligned — adjust(30.6, 1) would be 31, i.e. above the position.
    const exchange = buildExchange();
    exchange.fetchOpenPositions = vi.fn().mockResolvedValue([{ ...openPosition, contracts: 30.6 }]);

    const payload = await exchange.formatAlgoOrder({ ...sizedStop, amount: 45 } as any);

    expect(payload.planType).toBe('loss_plan');
    expect(payload.size).toBe('30');
  });

  it.each([
    [0.01, 0.29, 0.2934, '0.29'],
    [0.05, 1.15, 1.31, '1.15'],
    [0.1, 0.3, 0.44, '0.3'],
    [0.001, 99.999, 120, '99.999'],
  ])(
    'clamps exactly on a %p lot without dropping a whole lot to float error',
    async (step, contracts, requested, expected) => {
      // Math.floor(value / step) inherits IEEE division error — 0.29 / 0.01 is
      // 28.999999999999996, which floors to 0.28 and leaves a lot of the
      // position unstopped. Real bitget lots are fractional (ETH 0.01, XRP 0.1).
      const exchange = buildExchange();
      exchange.store.update({ markets: [{ ...market, precision: { amount: step, price: 0.00001 } }] as any });
      exchange.fetchOpenPositions = vi.fn().mockResolvedValue([{ ...openPosition, contracts }]);

      const payload = await exchange.formatAlgoOrder({ ...sizedStop, amount: requested } as any);

      expect(payload.planType).toBe('loss_plan');
      expect(payload.size).toBe(expected);
    },
  );

  it('falls back to a full-position stop when the position is smaller than one lot', async () => {
    // Clamping dust to 0 would POST size "0", which is not a sized stop at all.
    const exchange = buildExchange();
    exchange.fetchOpenPositions = vi.fn().mockResolvedValue([{ ...openPosition, contracts: 0.4 }]);

    const payload = await exchange.formatAlgoOrder({ ...sizedStop, amount: 30 } as any);

    expect(payload.planType).toBe('pos_loss');
    expect(payload.size).toBeUndefined();
  });

  it('does not cache an empty snapshot taken while flat', async () => {
    // placeOrders formats algo payloads before the entry order is placed, so a
    // bracket always takes one snapshot while flat. Caching it would downgrade
    // every sized stop for the rest of the TTL.
    const exchange = buildExchange();
    const fetchOpenPositions = vi.fn().mockResolvedValueOnce([]).mockResolvedValue([openPosition]);
    exchange.fetchOpenPositions = fetchOpenPositions;

    const flat = await exchange.formatAlgoOrder(sizedStop as any);
    expect(flat.planType).toBe('pos_loss');

    const filled = await exchange.formatAlgoOrder(sizedStop as any);
    expect(filled.planType).toBe('loss_plan');
    expect(filled.size).toBe('30');
    expect(fetchOpenPositions).toHaveBeenCalledTimes(2);
  });

  it('treats amount 0 as the full-position sentinel without any fetch', async () => {
    // amount === 0 is isFullStop's sentinel; sizing it from the position would
    // regress the "Full Position" display for every position-level TP/SL.
    const exchange = buildExchange();
    const fetchOpenPositions = vi.fn().mockResolvedValue([openPosition]);
    exchange.fetchOpenPositions = fetchOpenPositions;

    const payload = await exchange.formatAlgoOrder({ ...sizedStop, amount: 0 } as any);

    expect(payload.planType).toBe('pos_loss');
    expect(payload.size).toBeUndefined();
    expect(fetchOpenPositions).not.toHaveBeenCalled();
  });

  it('clamps down to a much smaller position rather than covering the whole of it', async () => {
    const exchange = buildExchange();
    exchange.fetchOpenPositions = vi.fn().mockResolvedValue([{ ...openPosition, contracts: 10 }]);

    const payload = await exchange.formatAlgoOrder(sizedStop as any);

    expect(payload.planType).toBe('loss_plan');
    expect(payload.size).toBe('10');
  });

  it('still falls back to a full-position stop when the protected side is flat', async () => {
    // The only remaining reason to downgrade: there is genuinely nothing to size
    // against. A stale opposite-side row must not count.
    const exchange = buildExchange();
    exchange.fetchOpenPositions = vi.fn().mockResolvedValue([{ ...openPosition, side: PositionSide.Short }]);

    const payload = await exchange.formatAlgoOrder(sizedStop as any);

    expect(payload.planType).toBe('pos_loss');
    expect(payload.size).toBeUndefined();
  });

  it('reports the downgrade when the snapshot fetch fails', async () => {
    // Degrading is deliberate (a throw would block the entry order in a
    // bracket), but it must not be silent — the stop covers the whole position
    // instead of the requested size.
    const exchange = buildExchange();
    exchange.fetchOpenPositions = vi.fn().mockRejectedValue(new Error('network'));
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    const payload = await exchange.formatAlgoOrder(sizedStop as any);

    expect(payload.planType).toBe('pos_loss');
    expect(emitAndLogError).toHaveBeenCalledWith(expect.stringContaining('full-position stop'));
  });

  it('resolves algo payloads before placing them in a batch', async () => {
    // placeOrders passed unresolved Promises into placeAlgoOrders, which read
    // planType off the Promise (undefined -> always the non-trailing endpoint)
    // and POSTed the Promise as the request body.
    const exchange = buildExchange();
    exchange.fetchOpenPositions = vi.fn().mockResolvedValue([openPosition]);
    const placeAlgoOrders = vi.fn().mockResolvedValue([]);
    exchange.placeAlgoOrders = placeAlgoOrders;
    exchange.placeOrderBatch = vi.fn().mockResolvedValue([]);

    await exchange.placeOrders([sizedStop] as any);

    expect(exchange.xhr.get).toHaveBeenCalledExactlyOnceWith(ENDPOINTS.ACCOUNT_SETTINGS_V3);
    const [payloads] = placeAlgoOrders.mock.calls[0];
    expect(payloads[0]).not.toBeInstanceOf(Promise);
    expect(payloads[0].planType).toBe('loss_plan');
    expect(payloads[0].size).toBe('30');
  });
});

describe('audit regression: Bitget cancel plan type after a partial close', () => {
  it('still computes loss_plan for a sized stop whose position has shrunk', async () => {
    // getCancelPlanType is synchronous and reads store.positions, so by cancel
    // time the position is often smaller than the size the stop was placed at.
    // Requiring contracts >= amount computed pos_loss for an order that was
    // placed as loss_plan, and bitget rejected the cancel.
    const exchange = buildExchange();
    exchange.store.update({ positions: [{ ...openPosition, contracts: 10 }] as any });

    const [planType] = (exchange as any).selectPlanTypeAndAmount(
      { ...sizedStop, amount: 30 },
      exchange.store.positions,
    );

    expect(planType).toBe('loss_plan');
  });
});
