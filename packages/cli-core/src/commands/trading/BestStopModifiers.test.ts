import { describe, it, expect, vi, beforeEach } from 'vitest';
import { BuyCommand } from './BuyCommand';
import { SellCommand } from './SellCommand';
import type { CommandContext } from '../Command';
import { OrderSide, OrderType } from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';

function makeExchange(): ExchangeLike {
  return {
    // A REAL venue id, not 'test': every write now goes through the shared
    // order policy, which asks `exchangeMetadata` about round-down, infinity
    // stops and the reduce-only clamp. A made-up name answers "unknown venue"
    // and no longer exercises the branches these tests are about.
    name: 'binance',
    store: {
      markets: [
        { symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } },
      ],
      tickers: [{ symbol: 'BTCUSDT', last: 50000, bid: 49995, ask: 50005 }],
      orders: [],
      positions: [],
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

describe('buy/sell `best` modifier', () => {
  it('buy <size> best places a limit at ticker.bid', async () => {
    const ex = makeExchange();
    await new BuyCommand().execute(ctx(['buy', '$1000', 'best'], ex));
    const order = lastOrder(ex);
    expect(order.type).toBe(OrderType.Limit);
    expect(order.side).toBe(OrderSide.Buy);
    expect(order.price).toBe(49995);
  });

  it('sell <size> best places a limit at ticker.ask', async () => {
    const ex = makeExchange();
    await new SellCommand().execute(ctx(['sell', '$1000', 'best'], ex));
    const order = lastOrder(ex);
    expect(order.type).toBe(OrderType.Limit);
    expect(order.side).toBe(OrderSide.Sell);
    expect(order.price).toBe(50005);
  });

  it('buy <size> best reduce honors reduce-only', async () => {
    const ex = makeExchange();
    await new BuyCommand().execute(ctx(['buy', '$1000', 'best', 'reduce'], ex));
    expect(lastOrder(ex).reduceOnly).toBe(true);
  });
});

describe('buy/sell `stop <p>` attached stop-loss', () => {
  it('attaches stopLoss to a market entry', async () => {
    const ex = makeExchange();
    await new BuyCommand().execute(ctx(['buy', '$1000', 'stop', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.type).toBe(OrderType.Market);
    expect(order.stopLoss).toBe(49000);
  });

  it('attaches stopLoss to a limit entry', async () => {
    const ex = makeExchange();
    await new BuyCommand().execute(ctx(['buy', '$1000', 'at', '49500', 'stop', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.type).toBe(OrderType.Limit);
    expect(order.price).toBe(49500);
    expect(order.stopLoss).toBe(49000);
  });

  it('attaches stopLoss to a best-quote entry', async () => {
    const ex = makeExchange();
    await new BuyCommand().execute(ctx(['buy', '$1000', 'best', 'stop', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.price).toBe(49995);
    expect(order.stopLoss).toBe(49000);
  });

  it('accepts a relative stop price (-N)', async () => {
    const ex = makeExchange();
    await new BuyCommand().execute(ctx(['buy', '$1000', 'stop', '-1000'], ex));
    expect(lastOrder(ex).stopLoss).toBe(49000); // 50000 - 1000
  });

  it('does not flag a forgotten-at error when stop <p> is present', async () => {
    const ex = makeExchange();
    const result = await new BuyCommand().execute(ctx(['buy', '$1000', 'stop', '49000'], ex));
    expect(result.success).toBe(true);
  });
});
