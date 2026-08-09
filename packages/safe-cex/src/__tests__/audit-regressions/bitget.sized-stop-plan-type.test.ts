import { describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '../../exchanges/bitget/bitget.exchange';
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

  it('falls back to a position-level stop when the order exceeds the position', async () => {
    const exchange = buildExchange();
    exchange.fetchOpenPositions = vi.fn().mockResolvedValue([{ ...openPosition, contracts: 10 }]);

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

    const [payloads] = placeAlgoOrders.mock.calls[0];
    expect(payloads[0]).not.toBeInstanceOf(Promise);
    expect(payloads[0].planType).toBe('loss_plan');
    expect(payloads[0].size).toBe('30');
  });
});
