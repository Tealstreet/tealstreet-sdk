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
   * `Position.realizedPnl` must NOT move this price, on ANY venue.
   *
   * It was briefly wired in behind a per-venue capability flag. That was
   * reverted: bitunix's is measured net of fees and funding in
   * `docs/POSITION_MODEL_VENUE_EVIDENCE_v1.md`, bybit's `curRealisedPnl` has
   * no evidence in this repo at all, and paradex's was inferred from the
   * field's name. Web does not read this field either — it derives banked PnL
   * from the user's own fills. Until the CLI can do the same, a present value
   * is ignored rather than trusted, and these two tests are what say so.
   */
  it('ignores realizedPnl, which is not trustworthy enough to place a stop on', async () => {
    const ex = makeExchange([pos({ realizedPnl: 1_000, contracts: 1 })]);
    await cmd.execute(ctx(['be'], ex));
    expect(placed(ex).triggerPrice).toBe(50_000);
  });

  it('is unmoved by a realizedPnl large enough to be a lifetime figure', async () => {
    const ex = makeExchange([pos({ realizedPnl: 500_000, contracts: 1 })]);
    await cmd.execute(ctx(['be'], ex));
    expect(placed(ex).triggerPrice).toBe(50_000);
  });

  /**
   * The offset is NOT side-aware — `+` is up the price axis for both sides.
   * Worth pinning because the docs used to call `+3%` "the good side", which
   * is only true for a long: on a short the same offset widens the stop.
   */
  it('moves a short\'s stop UP on +3%, the same direction as a long', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Short, entryPrice: 52_000, contracts: 1 })]);
    await cmd.execute(ctx(['be', '+3%'], ex));
    expect(placed(ex).triggerPrice).toBeCloseTo(53_560, 0);
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

  it('buys to close a short, at its entry', async () => {
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
