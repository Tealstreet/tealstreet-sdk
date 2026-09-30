import { describe, it, expect, vi, beforeEach } from 'vitest';
import { BreakevenCommand } from './BreakevenCommand';
import type { CommandContext } from '../Command';
import {
  OrderSide,
  OrderType,
  PositionSide,
  MarginType,
  PositionMode,
  type Order,
  type Position,
} from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';

function pos(p: Partial<Position> = {}): Position {
  return {
    id: 'pos1',
    symbol: 'BTCUSDT',
    side: PositionSide.Long,
    entryPrice: 50_000,
    notional: 50_000,
    leverage: 1,
    marginType: MarginType.Isolated,
    positionMode: PositionMode.OneWay,
    unrealizedPnl: 0,
    contracts: 1,
    liquidationPrice: 0,
    ...p,
  } as Position;
}

const order = (o: Partial<Order>): Order =>
  ({ id: 'o1', symbol: 'BTCUSDT', type: OrderType.StopLoss, side: OrderSide.Sell, ...o }) as Order;

function makeExchange(
  positions: Position[] = [],
  orders: Order[] = [],
  // `binance` never reports a position-scoped realizedPnl, so the DEFAULT
  // fixture is an untrusted venue. Every pre-existing expectation below is
  // therefore an assertion about the capability flag being absent.
  name = 'binance',
  market: Record<string, unknown> = {},
): ExchangeLike {
  return {
    name,
    store: {
      markets: [
        { symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 }, ...market },
      ],
      tickers: [{ symbol: 'BTCUSDT', last: 51_000, mark: 51_000 }],
      orders,
      positions,
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    placeOrder: vi.fn().mockResolvedValue(['order-1']),
    cancelOrders: vi.fn().mockResolvedValue(undefined),
  } as unknown as ExchangeLike;
}

function ctx(args: string[], exchange: ExchangeLike): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol: 'BTCUSDT',
    accountName: 'a',
    exchange,
    adapter: {
      printInfo: vi.fn(),
      print: vi.fn(),
      printError: vi.fn(),
      printSuccess: vi.fn(),
      printWarning: vi.fn(),
    },
  } as unknown as CommandContext;
}

const placed = (ex: ExchangeLike) => (ex.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];

describe('BreakevenCommand', () => {
  let cmd: BreakevenCommand;
  beforeEach(() => {
    cmd = new BreakevenCommand();
  });

  it('places a reduce-only stop at the entry price when nothing is banked', async () => {
    const ex = makeExchange([pos()]);
    const r = await cmd.execute(ctx(['be'], ex));
    expect(r.success).toBe(true);
    expect(placed(ex)).toMatchObject({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      triggerPrice: 50_000,
      reduceOnly: true,
    });
  });

  /**
   * `Position.realizedPnl` must NOT move this price on a venue that has not
   * been audited. It means different things per venue — lifetime on asterdex,
   * survives the close on lighter, funding and fees baked in on bitget, and
   * base-denominated against a quote price on every inverse contract. The
   * `reportsPositionRealizedPnl` metadata flag is what separates them, and
   * absence of the flag is the safe default these two tests pin.
   */
  it('ignores realizedPnl on a venue that does not report it position-scoped', async () => {
    const ex = makeExchange([pos({ realizedPnl: 1_000, contracts: 1 })]);
    await cmd.execute(ctx(['be'], ex));
    expect(placed(ex).triggerPrice).toBe(50_000);
  });

  it('is unmoved by a realizedPnl large enough to be a lifetime figure', async () => {
    const ex = makeExchange([pos({ realizedPnl: 500_000, contracts: 1 })]);
    await cmd.execute(ctx(['be'], ex));
    expect(placed(ex).triggerPrice).toBe(50_000);
  });

  it('applies an offset relative to break-even', async () => {
    const ex = makeExchange([pos()]);
    await cmd.execute(ctx(['be', '+1%'], ex));
    expect(placed(ex).triggerPrice).toBe(50_500);
  });

  /**
   * The obvious hand-written spelling is `cancel stops; stop $entry`, and
   * `cancel stops` treats anything that is not a limit or a market as a stop —
   * which sweeps up the whole TP ladder. Closing a user's targets to move
   * their stop would be worse than not having the command.
   */
  it('cancels the stop loss it replaces and leaves take-profits alone', async () => {
    const stop = order({ id: 'sl', type: OrderType.StopLoss, side: OrderSide.Sell });
    const tp1 = order({ id: 'tp1', type: OrderType.TakeProfit, side: OrderSide.Sell });
    const tp2 = order({ id: 'tp2', type: OrderType.TakeProfit, side: OrderSide.Sell });
    const ex = makeExchange([pos()], [stop, tp1, tp2]);

    await cmd.execute(ctx(['be'], ex));

    expect(ex.cancelOrders).toHaveBeenCalledWith([stop]);
    const cancelled = (ex.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
    expect(cancelled.map((o) => o.id)).not.toContain('tp1');
    expect(cancelled.map((o) => o.id)).not.toContain('tp2');
  });

  it('buys to close a short, at its entry on an unflagged venue', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Short, realizedPnl: 1_000, contracts: 1 })]);
    await cmd.execute(ctx(['be'], ex));
    expect(placed(ex)).toMatchObject({ side: OrderSide.Buy, triggerPrice: 50_000 });
  });

  // Same rule as `stop`: what the order IS depends on which side of the mark it
  // sits, because that is when it fires. Mark is 51,000 here.
  it('is a take-profit when the price sits beyond the mark', async () => {
    const ex = makeExchange([pos({ entryPrice: 52_000, contracts: 1 })]);
    await cmd.execute(ctx(['be'], ex));
    expect(placed(ex)).toMatchObject({ triggerPrice: 52_000, type: OrderType.TakeProfit });
  });

  it('refuses when there is no position', async () => {
    const r = await cmd.execute(ctx(['be'], makeExchange()));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/No position/);
  });

  /**
   * The other half of the flag. Bybit reports `curRealisedPnl` off the position
   * payload, per position cycle, in the quote — so banking $1,000 on a
   * 1-contract long from 50,000 means you can now exit at 49,000 and still be
   * flat overall. This is the behaviour jrnats was told `be` had.
   */
  it('moves a long DOWN by the banked amount on a venue that reports it', async () => {
    const ex = makeExchange([pos({ realizedPnl: 1_000, contracts: 1 })], [], 'bybit');
    await cmd.execute(ctx(['be'], ex));
    expect(placed(ex).triggerPrice).toBe(49_000);
  });

  it('moves a short UP by the banked amount, the same profit in the other direction', async () => {
    const ex = makeExchange(
      [pos({ side: PositionSide.Short, realizedPnl: 1_000, contracts: 1 })],
      [],
      'bybit',
    );
    await cmd.execute(ctx(['be'], ex));
    expect(placed(ex).triggerPrice).toBe(51_000);
  });

  it('divides the banked amount across the contracts still open', async () => {
    const ex = makeExchange([pos({ realizedPnl: 1_000, contracts: 4 })], [], 'bybit');
    await cmd.execute(ctx(['be'], ex));
    expect(placed(ex).triggerPrice).toBe(49_750);
  });

  /**
   * bybitdemo and hyrotrader spread bybit's metadata and run the same mapper.
   * hyrotrader is the prop-firm venue the reporter actually trades on, so this
   * is the fixture that matches the report rather than a proxy for it.
   */
  it('trusts the bybit variants that inherit the flag', async () => {
    for (const venue of ['bybitdemo', 'hyrotrader']) {
      const ex = makeExchange([pos({ realizedPnl: 1_000, contracts: 1 })], [], venue);
      await cmd.execute(ctx(['be'], ex));
      expect(placed(ex).triggerPrice).toBe(49_000);
    }
  });

  /**
   * An inverse contract's realised figure is denominated in the BASE while the
   * entry price is in the quote, so subtracting one from the other is a unit
   * error that would place a real stop at a nonsense price. The venues are
   * flagged false individually; this guard is what survives someone adding a
   * new inverse venue by spreading its linear sibling's metadata.
   */
  it('refuses the banked figure on an inverse market even if the venue is flagged', async () => {
    const ex = makeExchange([pos({ realizedPnl: 1_000, contracts: 1 })], [], 'bybit', { isInverse: true });
    await cmd.execute(ctx(['be'], ex));
    expect(placed(ex).triggerPrice).toBe(50_000);
  });

  /**
   * `getExchangeMetadata` THROWS on a name it does not know, which is why the
   * capability lookup reads the map directly with `?.` instead. That choice is
   * what this pins: an unrecognised venue must not fail in the BREAK-EVEN
   * calculation.
   *
   * It still cannot place an order — `prepareOrder` calls
   * `venueSupportsInfinityStops`, which goes through `getExchangeMetadata` and
   * throws first. That is pre-existing and shared by every order-placing
   * command, so the assertion is on WHERE it fails, not that it succeeds.
   */
  it('does not fail in the break-even lookup on an unregistered venue', async () => {
    const ex = makeExchange([pos({ realizedPnl: 1_000, contracts: 1 })], [], 'not-a-real-venue');
    await expect(cmd.execute(ctx(['be'], ex))).rejects.toThrow(/Unknown exchange metadata/);
    expect(ex.placeOrder).not.toHaveBeenCalled();
  });

  /**
   * A market the store has not loaded is not evidence of a LINEAR market, so
   * the guard is written `market ? market.isInverse !== true : false` rather
   * than the natural-looking `!market?.isInverse`, which reads the same and
   * fails the other way — an absent market makes it TRUE and the banked figure
   * would be used unverified.
   *
   * The guard is defence in depth rather than a live path: `prepareCliOrder`
   * refuses a symbol with no market before any order is built, so this asserts
   * that no order is placed at all.
   */
  it('places nothing at all when the market is not in the store', async () => {
    const ex = makeExchange([pos({ realizedPnl: 1_000, contracts: 1 })], [], 'bybit');
    (ex.store as { markets: unknown[] }).markets = [];
    await expect(cmd.execute(ctx(['be'], ex))).rejects.toThrow(/Market not found/);
    expect(ex.placeOrder).not.toHaveBeenCalled();
  });

  /**
   * The order type is chosen by which side of the mark the price lands on, so
   * an underwater position gets a take-profit — and this command has just
   * cancelled the stop it was replacing. Reporting only "Stop moved" would
   * claim protection the position does not have.
   */
  it('warns that no stop remains when break-even lands beyond the market', async () => {
    const ex = makeExchange([pos({ entryPrice: 52_000, contracts: 1 })]);
    const c = ctx(['be'], ex);
    await cmd.execute(c);
    expect(placed(ex).type).toBe(OrderType.TakeProfit);
    expect(c.adapter.printWarning).toHaveBeenCalledWith(expect.stringContaining('no stop loss'));
  });

  it('says nothing of the sort when it really is a stop', async () => {
    const ex = makeExchange([pos()]);
    const c = ctx(['be'], ex);
    await cmd.execute(c);
    expect(placed(ex).type).toBe(OrderType.StopLoss);
    expect(c.adapter.printWarning).not.toHaveBeenCalled();
  });

  it('refuses rather than guessing when both sides are open', async () => {
    const ex = makeExchange([pos({ id: 'l' }), pos({ id: 's', side: PositionSide.Short })]);
    const r = await cmd.execute(ctx(['be'], ex));
    expect(r.success).toBe(false);
    expect(ex.placeOrder).not.toHaveBeenCalled();
  });
});
