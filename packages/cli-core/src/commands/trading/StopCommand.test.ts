import { describe, it, expect, vi, beforeEach } from 'vitest';
import { StopCommand } from './StopCommand';
import type { CommandContext } from '../Command';
import {
  OrderSide,
  OrderType,
  PositionSide,
  MarginType,
  PositionMode,
  type Position,
} from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';

function pos(p: Partial<Position> = {}): Position {
  return {
    id: p.id ?? 'pos1',
    symbol: p.symbol ?? 'BTCUSDT',
    side: p.side ?? PositionSide.Long,
    entryPrice: p.entryPrice ?? 50_000,
    notional: p.notional ?? 50_000,
    leverage: p.leverage ?? 1,
    marginType: p.marginType ?? MarginType.Isolated,
    positionMode: p.positionMode ?? PositionMode.OneWay,
    unrealizedPnl: p.unrealizedPnl ?? 0,
    contracts: p.contracts ?? 1,
    liquidationPrice: p.liquidationPrice ?? 0,
  } as Position;
}

function makeExchange(positions: Position[] = []): ExchangeLike {
  return {
    // A REAL venue id, not 'test': every write now goes through the shared
    // order policy, which asks `exchangeMetadata` about round-down, infinity
    // stops and the reduce-only clamp. A made-up name answers "unknown venue"
    // and no longer exercises the branches these tests are about.
    name: 'binance',
    store: {
      markets: [
        { symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } },
        { symbol: 'ETHUSDT', base: 'ETH', quote: 'USDT', precision: { amount: 0.01, price: 0.01 } },
      ],
      tickers: [
        { symbol: 'BTCUSDT', last: 50_000, mark: 50_000 },
        { symbol: 'ETHUSDT', last: 3_000, mark: 3_000 },
      ],
      orders: [],
      positions,
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    placeOrder: vi.fn().mockResolvedValue(['order-1']),
  } as unknown as ExchangeLike;
}

function ctx(args: string[], exchange: ExchangeLike, symbol = 'BTCUSDT'): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol,
    accountName: 'a',
    exchange,
    adapter: {
      print: vi.fn(),
      printInfo: vi.fn(),
      printError: vi.fn(),
      printSuccess: vi.fn(),
      printWarning: vi.fn(),
      listAccounts: vi.fn().mockReturnValue([]),
      getAccount: vi.fn(),
      addAccount: vi.fn(),
      updateAccount: vi.fn(),
      removeAccount: vi.fn(),
      setDefaultAccount: vi.fn(),
      getDefaultSymbol: vi.fn(),
      setDefaultSymbol: vi.fn(),
      getActiveAccounts: vi.fn().mockReturnValue([]),
      createExchange: vi.fn(),
      startExchange: vi.fn(),
      stopExchange: vi.fn(),
    },
  } as unknown as CommandContext;
}

function lastOrder(ex: ExchangeLike) {
  const calls = (ex.placeOrder as ReturnType<typeof vi.fn>).mock.calls;
  return calls[calls.length - 1][0];
}

function nthOrder(ex: ExchangeLike, n: number) {
  const calls = (ex.placeOrder as ReturnType<typeof vi.fn>).mock.calls;
  return calls[n][0];
}

describe('StopCommand — preconditions', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('errors when not connected', async () => {
    const r = await cmd.execute(ctx(['stop', 'at', '49000'], null as unknown as ExchangeLike));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Not connected/);
  });

  it("errors when exchange doesn't support placeOrder", async () => {
    const ex = makeExchange([pos()]);
    delete (ex as unknown as { placeOrder?: unknown }).placeOrder;
    const r = await cmd.execute(ctx(['stop', 'at', '49000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/does not support/);
  });

  it('errors when there is no position to stop', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['stop', 'at', '49000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/No position/);
  });

  it('errors when ticker price is unavailable', async () => {
    const ex = makeExchange([pos()]);
    ex.store!.tickers = [];
    const r = await cmd.execute(ctx(['stop', 'at', '49000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Current price is not available/);
  });

  /**
   * The message used to be the bare string "Not implemented", which reads as
   * "this command does not exist yet" rather than "I could not find a price in
   * what you typed" -- so it must name the expected form. Reported from Discord
   * 2026-09-29 by a user who could not set a stop at all.
   */
  it('names the expected form when there is no price in the command', async () => {
    const ex = makeExchange([pos()]);
    const r = await cmd.execute(ctx(['stop', 'forty', 'nine'], ex));
    expect(r.success).toBe(false);
    expect(r.error).not.toBe('Not implemented');
    expect(r.error).toMatch(/could not find a trigger price/);
    // Echoes what they typed and shows a form that works.
    expect(r.error).toContain('stop forty nine');
    expect(r.error).toContain('stop 18500');
  });
});

/**
 * `looksLikePrice` accepted `1%` but not `0.1%`, because the percent branch was
 * a separate `\d+%` alternative with no decimal part. `stop at 0.1%` is this
 * command's own first documented example, and it threw
 * "Price 0.1% does not look like a price". A fractional percent is the normal
 * case for a tight stop.
 */
describe('StopCommand — fractional percentages', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  const placed = (ex: ExchangeLike) => (ex.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];

  it('accepts the command\'s own first documented example', async () => {
    const ex = makeExchange([pos()]);
    const r = await cmd.execute(ctx(['stop', 'at', '0.1%'], ex));
    expect(r.success).toBe(true);
    expect(placed(ex)).toMatchObject({ triggerPrice: 50_050 });
  });

  it('accepts a fractional percent below the mark, as a real stop loss', async () => {
    const ex = makeExchange([pos()]);
    const r = await cmd.execute(ctx(['stop', '-0.25%'], ex));
    expect(r.success).toBe(true);
    expect(placed(ex)).toMatchObject({ type: OrderType.StopLoss, triggerPrice: 49_875 });
  });

  // Widening the regex must not make it accept words. `processPrice` throws
  // here rather than returning a failed result; the executor catches it and
  // shows the message, so the user still learns what was wrong.
  it('still rejects something that is not a price', async () => {
    const ex = makeExchange([pos()]);
    await expect(cmd.execute(ctx(['stop', 'at', 'forty'], ex))).rejects.toThrow(/does not look like a price/);
    expect(ex.placeOrder).not.toHaveBeenCalled();
  });
});

/**
 * `at` is optional. Every documented example carried it, so a bare trigger
 * price -- the obvious thing to type -- fell through to the catch-all error.
 * A bare number can never be a size, because `looksLikeSize` requires a `$`,
 * so there is no ambiguity to resolve.
 */
describe('StopCommand — `at` is optional', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  const placed = (ex: ExchangeLike) => (ex.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];

  it('accepts a bare trigger price', async () => {
    const ex = makeExchange([pos()]);
    const r = await cmd.execute(ctx(['stop', '49000'], ex));
    expect(r.success).toBe(true);
    expect(placed(ex)).toMatchObject({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      triggerPrice: 49_000,
      reduceOnly: true,
    });
  });

  it('places the identical order with or without `at`', async () => {
    const bare = makeExchange([pos()]);
    await cmd.execute(ctx(['stop', '49000'], bare));
    const withAt = makeExchange([pos()]);
    await cmd.execute(ctx(['stop', 'at', '49000'], withAt));
    expect(placed(bare)).toEqual(placed(withAt));
  });

  it('accepts a bare price after an explicit side', async () => {
    const ex = makeExchange([pos()]);
    const r = await cmd.execute(ctx(['stop', 'sell', '49000'], ex));
    expect(r.success).toBe(true);
    expect(placed(ex)).toMatchObject({ side: OrderSide.Sell, triggerPrice: 49_000 });
  });

  it('accepts a bare price after a size, without mistaking the size for it', async () => {
    const ex = makeExchange([pos()]);
    const r = await cmd.execute(ctx(['stop', '$500', '49000'], ex));
    expect(r.success).toBe(true);
    expect(placed(ex)).toMatchObject({ triggerPrice: 49_000 });
    expect(placed(ex).amount).toBeGreaterThan(0);
  });

  it('still reads a relative price and a symbol override without `at`', async () => {
    const ex = makeExchange([pos({ symbol: 'ETHUSDT' })]);
    const r = await cmd.execute(ctx(['stop', '-1%', 'ETHUSDT'], ex));
    expect(r.success).toBe(true);
    expect(placed(ex)).toMatchObject({ symbol: 'ETHUSDT', triggerPrice: 2_970 });
  });

  // The trigger keyword still separates trigger from limit with no `at`.
  it('supports a limit stop without `at`', async () => {
    const ex = makeExchange([pos()]);
    const r = await cmd.execute(ctx(['stop', '49000', 'trigger', '49100'], ex));
    expect(r.success).toBe(true);
    expect(placed(ex)).toMatchObject({ price: 49_000, triggerPrice: 49_100 });
  });
});

describe('StopCommand — market stops on a single position', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('absolute price below mark on a long → StopLoss', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long, contracts: 1 })]);
    await cmd.execute(ctx(['stop', 'at', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.symbol).toBe('BTCUSDT');
    expect(order.side).toBe(OrderSide.Sell); // long → sell to exit
    expect(order.type).toBe(OrderType.StopLoss);
    expect(order.reduceOnly).toBe(true);
    expect(order.price).toBe(49000);
    expect(order.triggerPrice).toBe(49000); // market stop: limit == trigger
    expect(order.amount).toBe(0); // unsized → use position
  });

  it('absolute price above mark on a long → TakeProfit', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', 'at', '52000'], ex));
    expect(lastOrder(ex).type).toBe(OrderType.TakeProfit);
  });

  it('absolute price above mark on a short → StopLoss', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Short, contracts: 0.5 })]);
    await cmd.execute(ctx(['stop', 'at', '52000'], ex));
    const order = lastOrder(ex);
    expect(order.side).toBe(OrderSide.Buy); // short → buy to exit
    expect(order.type).toBe(OrderType.StopLoss);
  });

  it('absolute price below mark on a short → TakeProfit', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Short })]);
    await cmd.execute(ctx(['stop', 'at', '48000'], ex));
    expect(lastOrder(ex).type).toBe(OrderType.TakeProfit);
  });

  it('relative price (-100) becomes absolute via processPrice', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', 'at', '-100'], ex));
    expect(lastOrder(ex).price).toBe(49900); // 50000 - 100
  });

  it('percent (-2%) becomes absolute via processPrice', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', 'at', '-2%'], ex));
    expect(lastOrder(ex).price).toBe(49000); // 50000 - 2%
  });

  it('explicit symbol scoping picks the named instrument', async () => {
    const ex = makeExchange([pos({ symbol: 'ETHUSDT', side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', 'at', '2900', 'ETHUSDT'], ex));
    const order = lastOrder(ex);
    expect(order.symbol).toBe('ETHUSDT');
    expect(order.price).toBe(2900);
  });
});

describe('StopCommand — sized stops (partial)', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('"$500 at 1%" places a sized stop at +1%', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', '$500', 'at', '1%'], ex));
    const order = lastOrder(ex);
    expect(order.price).toBe(50500); // +1%
    // $500 / 50500 rounds to 0.01 at the 0.001 lot step.
    expect(order.amount).toBe(0.01);
  });

  it('sizes USD at the stop price, not the trigger or last price', async () => {
    const ex = makeExchange([pos()]);
    ex.store!.tickers![0].last = 80_000;
    ex.store!.tickers![0].mark = 80_000;

    const result = await cmd.execute(ctx(['stop', '600$', 'at', '60000', 'trigger', '65000'], ex));

    expect(result.success).toBe(true);
    expect(lastOrder(ex)).toMatchObject({ price: 60_000, triggerPrice: 65_000, amount: 0.01 });
  });

  it('"500$ at -1%" suffix form also resolves', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', '500$', 'at', '-1%'], ex));
    expect(lastOrder(ex).amount).toBe(0.01);
  });
});

describe('StopCommand — hedge mode disambiguation', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('errors when both sides are open and no buy/sell specified', async () => {
    const ex = makeExchange([
      pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
    ]);
    const r = await cmd.execute(ctx(['stop', 'at', '49000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/More than one position/);
    expect(ex.placeOrder).not.toHaveBeenCalled();
  });

  it('"stop buy at <p>" targets the short position', async () => {
    const ex = makeExchange([
      pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
    ]);
    await cmd.execute(ctx(['stop', 'buy', 'at', '52000'], ex));
    const order = lastOrder(ex);
    // 'buy' side means we're closing the SHORT position (short → buy to exit).
    expect(order.side).toBe(OrderSide.Buy);
  });

  it('"stop sell at <p>" targets the long position', async () => {
    const ex = makeExchange([
      pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
    ]);
    await cmd.execute(ctx(['stop', 'sell', 'at', '49000'], ex));
    expect(lastOrder(ex).side).toBe(OrderSide.Sell);
  });

  it('"stop sell $500 at -1%" — sized hedge stop on the long', async () => {
    const ex = makeExchange([
      pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
    ]);
    await cmd.execute(ctx(['stop', 'sell', '$500', 'at', '-1%'], ex));
    const order = lastOrder(ex);
    expect(order.side).toBe(OrderSide.Sell);
    expect(order.amount).toBe(0.01);
    expect(order.price).toBe(49500);
  });
});

describe('StopCommand — Phase 3: limit stops via `trigger <abs>`', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('"trigger <abs>" makes `at` the limit price and trigger the trigger', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    // Long, mark=50000. `at -100` → limit = 49900. `trigger 49500` →
    // trigger fires when price <= 49500 (below mark = stop loss).
    await cmd.execute(ctx(['stop', 'at', '-100', 'trigger', '49500'], ex));
    const order = lastOrder(ex);
    expect(order.price).toBe(49900); // limit
    expect(order.triggerPrice).toBe(49500); // trigger (absolute)
    expect(order.type).toBe(OrderType.StopLoss); // trigger < mark
  });

  it('TP/SL classification uses trigger, not limit', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    // limit price below mark, but trigger ABOVE mark → TakeProfit
    await cmd.execute(ctx(['stop', 'at', '49500', 'trigger', '52000'], ex));
    expect(lastOrder(ex).type).toBe(OrderType.TakeProfit);
  });

  it('errors when `trigger` has no argument', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    const r = await cmd.execute(ctx(['stop', 'at', '49000', 'trigger'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/trigger: missing/);
  });

  it.each([
    ['abc', /must be an absolute positive price/],
    ['-1000', /must be an absolute positive price/],
    ['0', /must be an absolute positive price/],
  ])('rejects malformed trigger value "%s"', async (badArg, pattern) => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    const r = await cmd.execute(ctx(['stop', 'at', '49000', 'trigger', badArg], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(pattern);
  });

  it('locks in: trigger is absolute-only — relative forms are NOT applied to it', async () => {
    // The trigger arg is parseFloat'd directly, not through processPrice.
    // `+100` looks like a number in some parsers, but parseFloat('+100')
    // returns 100 — accepted as absolute. Verify that path doesn't
    // accidentally re-anchor against current price.
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', 'at', '49000', 'trigger', '+100'], ex));
    expect(lastOrder(ex).triggerPrice).toBe(100); // absolute, NOT 50100
  });

  it('sized trigger stop carries amount through', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', '$500', 'at', '-1%', 'trigger', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.amount).toBe(0.01);
    expect(order.price).toBe(49500);
    expect(order.triggerPrice).toBe(49000);
  });

  it('hedge mode trigger stop on the long side', async () => {
    const ex = makeExchange([
      pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
    ]);
    await cmd.execute(ctx(['stop', 'sell', 'at', '-1%', 'trigger', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.side).toBe(OrderSide.Sell);
    expect(order.triggerPrice).toBe(49000);
  });
});

describe('StopCommand — multi-order paths', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('places exactly one order for a single position', async () => {
    const ex = makeExchange([pos()]);
    await cmd.execute(ctx(['stop', 'at', '49000'], ex));
    expect(ex.placeOrder).toHaveBeenCalledTimes(1);
  });

  it('"stop buy at" only fires for the short side, "stop sell at" only fires for the long side', async () => {
    // Same context, two separate command runs — verify each side
    // hits its own position when both are open.
    const ex = makeExchange([
      pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
    ]);
    await cmd.execute(ctx(['stop', 'sell', 'at', '49000'], ex));
    await cmd.execute(ctx(['stop', 'buy', 'at', '52000'], ex));
    expect(ex.placeOrder).toHaveBeenCalledTimes(2);
    expect(nthOrder(ex, 0).side).toBe(OrderSide.Sell);
    expect(nthOrder(ex, 1).side).toBe(OrderSide.Buy);
  });
});

describe('StopCommand — suffix-USD size regression', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('e2e: `stop 1000$ at 49000` (suffix size) places a sized stop on the focused symbol', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long, contracts: 1 })]);
    ex.store!.markets![0].precision.amount = 0.000001;
    await cmd.execute(ctx(['stop', '1000$', 'at', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.symbol).toBe('BTCUSDT');
    expect(order.price).toBe(49000);
    // $1000 / 49000, rounded to the 0.000001 lot step.
    expect(order.amount).toBe(0.020408);
  });

  it('e2e: `stop 1000$ at 49000` with a base="1000" market still uses focused symbol', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long, contracts: 1 })]);
    ex.store!.markets = [
      ...(ex.store!.markets ?? []),
      { symbol: '1000USDT', base: '1000', quote: 'USDT', precision: { amount: 1, price: 0.0001 } } as never,
    ];
    ex.store!.tickers = [
      ...(ex.store!.tickers ?? []),
      { symbol: '1000USDT', last: 0.5 } as never,
    ];
    await cmd.execute(ctx(['stop', '1000$', 'at', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.symbol).toBe('BTCUSDT');
    expect(order.amount).toBeCloseTo(0.02);
  });
});
