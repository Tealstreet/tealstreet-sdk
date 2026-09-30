import { describe, it, expect, vi, beforeEach } from 'vitest';
import { TrailCommand } from './TrailCommand';
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
    id: 'pos1',
    symbol: 'BTCUSDT',
    side: PositionSide.Long,
    entryPrice: 50_000,
    notional: 50_000,
    leverage: 10,
    marginType: MarginType.Isolated,
    positionMode: PositionMode.OneWay,
    unrealizedPnl: 0,
    contracts: 1,
    liquidationPrice: 0,
    ...p,
  } as Position;
}

function makeExchange(positions: Position[] = [], name = 'bybit'): ExchangeLike {
  return {
    name,
    store: {
      markets: [{ symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } }],
      tickers: [{ symbol: 'BTCUSDT', last: 60_000, mark: 60_000 }],
      orders: [],
      positions,
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    placeOrder: vi.fn().mockResolvedValue(['order-1']),
  } as unknown as ExchangeLike;
}

function ctx(args: string[], exchange: ExchangeLike): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol: 'BTCUSDT',
    accountName: 'a',
    exchange,
    adapter: { printInfo: vi.fn(), print: vi.fn(), printError: vi.fn(), printSuccess: vi.fn() },
  } as unknown as CommandContext;
}

const placed = (ex: ExchangeLike) => (ex.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];

describe('TrailCommand', () => {
  let cmd: TrailCommand;
  beforeEach(() => {
    cmd = new TrailCommand();
  });

  // Distance is from the CURRENT price, not entry — that is what a trailing
  // stop means to the venue. Entry is 50k, last is 60k.
  it('places the stop a percentage behind the current price, not the entry', async () => {
    const ex = makeExchange([pos()]);
    const r = await cmd.execute(ctx(['trail', '20%'], ex));
    expect(r.success).toBe(true);
    expect(placed(ex)).toMatchObject({
      side: OrderSide.Sell,
      type: OrderType.TrailingStopLoss,
      reduceOnly: true,
      price: 48_000,
    });
  });

  it('takes an absolute distance', async () => {
    const ex = makeExchange([pos()]);
    await cmd.execute(ctx(['trail', '500'], ex));
    expect(placed(ex).price).toBe(59_500);
  });

  it('puts a short stop ABOVE the market and buys to close', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Short })]);
    await cmd.execute(ctx(['trail', '20%'], ex));
    expect(placed(ex)).toMatchObject({ side: OrderSide.Buy, price: 72_000 });
  });

  /**
   * Only bybit and woo implement placeTrailingStopLoss. Naming the venue beats
   * letting it reject an order for an endpoint it does not have.
   */
  it('refuses by name on a venue with no trailing support', async () => {
    const ex = makeExchange([pos()], 'okx');
    const r = await cmd.execute(ctx(['trail', '20%'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/okx does not support trailing stops/);
    expect(ex.placeOrder).not.toHaveBeenCalled();
  });

  it('names the expected form when no distance is given', async () => {
    const r = await cmd.execute(ctx(['trail'], makeExchange([pos()])));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/trail 20%/);
  });

  it('refuses when there is no position', async () => {
    const r = await cmd.execute(ctx(['trail', '20%'], makeExchange()));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/No position/);
  });

  it('refuses a distance that would put the stop below zero', async () => {
    const ex = makeExchange([pos()]);
    const r = await cmd.execute(ctx(['trail', '150%'], ex));
    expect(r.success).toBe(false);
    expect(ex.placeOrder).not.toHaveBeenCalled();
  });
});
