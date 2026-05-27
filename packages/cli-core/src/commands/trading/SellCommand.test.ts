import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SellCommand } from './SellCommand';
import type { CommandContext } from '../Command';
import { OrderSide, OrderType, OrderTimeInForce } from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';

// Helper to create mock exchange
function createMockExchange(overrides: Partial<ExchangeLike> = {}): ExchangeLike {
  return {
    name: 'test-exchange',
    store: {
      markets: [
        { symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } },
        { symbol: 'ETHUSDT', base: 'ETH', quote: 'USDT', precision: { amount: 0.01, price: 0.01 } },
        { symbol: 'WIFUSDT', base: 'WIF', quote: 'USDT', precision: { amount: 1, price: 0.0001 } },
      ],
      tickers: [
        { symbol: 'BTCUSDT', last: 50000 },
        { symbol: 'ETHUSDT', last: 3000 },
        { symbol: 'WIFUSDT', last: 2.5 },
      ],
      orders: [],
      positions: [],
    },
    emitter: {
      on: vi.fn(),
      off: vi.fn(),
      emit: vi.fn(),
    },
    placeOrder: vi.fn().mockResolvedValue(['order-123']),
    ...overrides,
  } as unknown as ExchangeLike;
}

// Helper to create mock context
function createMockContext(
  args: string[],
  exchange: ExchangeLike | null = null,
  symbol = 'BTCUSDT',
): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol,
    accountName: 'test-account',
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
  };
}

describe('SellCommand', () => {
  let command: SellCommand;

  beforeEach(() => {
    command = new SellCommand();
  });

  describe('command properties', () => {
    it('has correct name', () => {
      expect(command.name).toBe('sell');
    });

    it('has correct aliases', () => {
      expect(command.aliases).toContain('s');
    });

    it('has correct description', () => {
      expect(command.description).toBe('Sell market or limit order');
    });

    it('requires connection', () => {
      expect(command.requiresConnection).toBe(true);
    });

    it('returns Sell side', () => {
      expect(command['side']).toBe(OrderSide.Sell);
    });
  });

  describe('error handling', () => {
    it('returns error when not connected to exchange', async () => {
      const ctx = createMockContext(['sell', '$100']);
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Not connected to exchange');
    });

    it('returns error when exchange does not support placing orders', async () => {
      const exchange = createMockExchange({ placeOrder: undefined });
      const ctx = createMockContext(['sell', '$100'], exchange);
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Exchange does not support placing orders');
    });

    it('returns error when size argument is missing', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell'], exchange);
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toContain('Size argument missing');
    });
  });

  describe('market orders', () => {
    it('places market sell order with session symbol', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '$100'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'BTCUSDT',
          side: OrderSide.Sell,
          type: OrderType.Market,
          reduceOnly: false,
          timeInForce: OrderTimeInForce.GoodTillCancel,
        }),
      );
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith('Order placed');
    });

    it('places market sell order with symbol at start', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', 'ETHUSDT', '$300'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'ETHUSDT',
          side: OrderSide.Sell,
          type: OrderType.Market,
        }),
      );
    });

    it('places market sell order with symbol at end', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '$300', 'ETHUSDT'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'ETHUSDT',
          side: OrderSide.Sell,
          type: OrderType.Market,
        }),
      );
    });

    it('places reduce-only market sell order', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '$100', 'reduce'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'BTCUSDT',
          side: OrderSide.Sell,
          type: OrderType.Market,
          reduceOnly: true,
        }),
      );
    });

    it('places orders for multiple symbols', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', 'BTCUSDT,ETHUSDT', '$100'], exchange);
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(2);
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith('Orders placed');
    });

    it('detects bare numbers that look like prices (safety check)', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '$100', '50000'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Invalid syntax');
      expect(ctx.adapter.printError).toHaveBeenCalledWith(
        expect.stringContaining('did you mean'),
      );
    });
  });

  describe('limit orders', () => {
    it('places limit sell order with "at" keyword', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '$100', 'at', '51000'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'BTCUSDT',
          side: OrderSide.Sell,
          type: OrderType.Limit,
          price: 51000,
          reduceOnly: false,
        }),
      );
    });

    it('places limit sell order with symbol at start', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', 'ETHUSDT', '$300', 'at', '3100'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'ETHUSDT',
          side: OrderSide.Sell,
          type: OrderType.Limit,
          price: 3100,
        }),
      );
    });

    it('places limit sell order with symbol at end', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '$300', 'at', '3100', 'ETHUSDT'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'ETHUSDT',
          side: OrderSide.Sell,
          type: OrderType.Limit,
          price: 3100,
        }),
      );
    });

    it('returns error when price argument is missing after "at"', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '$100', 'at'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Price argument missing after "at" keyword');
    });

    it('handles relative price with + prefix', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '$100', 'at', '+500'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          price: 50500, // 50000 + 500
        }),
      );
    });

    it('handles percentage price', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '$100', 'at', '+5%'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          price: 52500, // 50000 + 5%
        }),
      );
    });
  });

  describe('size conversion', () => {
    it('converts USD notional to contract size', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '$500'], exchange, 'BTCUSDT');
      await command.execute(ctx);

      // $500 / $50000 per BTC = 0.01 BTC, adjusted for precision 0.001
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          amount: 0.01,
        }),
      );
    });

    it('end-to-end: `sell 1000$` (suffix form) places an order with the right amount', async () => {
      // Same regression as BuyCommand — sell shares the path through
      // BaseBuySellCommand and TradingBaseCommand.validateSymbols, but
      // worth its own e2e check so the suffix form has direct sell
      // coverage too.
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '1000$'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('BTCUSDT');
      expect(order.side).toBe(OrderSide.Sell);
      expect(order.type).toBe(OrderType.Market);
      expect(order.amount).toBe(0.02);
      expect(order.amount).toBeGreaterThan(0);
    });

    it('end-to-end: `sell 100$` suffix form', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '100$'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.amount).toBe(0.002);
    });

    it('end-to-end: `sell 1000$ ETHUSDT` with trailing instrument', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '1000$', 'ETHUSDT'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('ETHUSDT');
      expect(order.amount).toBe(0.33);
    });

    it('end-to-end: `sell ETHUSDT 1000$` with leading instrument', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', 'ETHUSDT', '1000$'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('ETHUSDT');
      expect(order.amount).toBe(0.33);
    });

    it('end-to-end: `sell 1000$ at 51000` limit form with suffix size', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', '1000$', 'at', '51000'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.type).toBe(OrderType.Limit);
      expect(order.price).toBe(51000);
      expect(order.amount).toBe(0.02);
    });

    it('end-to-end: `sell 1000$ best` best-quote with suffix size', async () => {
      const exchange = createMockExchange();
      // Best needs bid/ask on the ticker.
      exchange.store!.tickers = [
        { symbol: 'BTCUSDT', last: 50000, bid: 49995, ask: 50005 } as never,
      ];
      const ctx = createMockContext(['sell', '1000$', 'best'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.type).toBe(OrderType.Limit);
      // sell `best` = ticker.ask
      expect(order.price).toBe(50005);
      expect(order.amount).toBeGreaterThan(0);
    });

    it('e2e: `sell 1000$` is NOT misidentified as a 1000PEPEUSDT-style symbol', async () => {
      // Mirror of the BuyCommand regression — sell path also runs
      // through validateSymbols + transformSymbol's fuzzy base-prefix
      // matching. Lock in the same contract.
      const exchange = createMockExchange();
      exchange.store!.markets = [
        ...exchange.store!.markets!,
        { symbol: '1000PEPEUSDT', base: '1000PEPE', quote: 'USDT', precision: { amount: 1, price: 0.000001 } } as never,
      ];
      exchange.store!.tickers = [
        ...exchange.store!.tickers!,
        { symbol: '1000PEPEUSDT', last: 0.001 } as never,
      ];
      const ctx = createMockContext(['sell', '1000$'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('BTCUSDT');
      expect(order.amount).toBe(0.02);
    });

    it('e2e: `sell 1000$` with a base="1000" market — the actual user repro', async () => {
      // Same root-cause check as in BuyCommand. With base="1000",
      // `'1000$'.startsWith('1000')` is true → without the fix the
      // symbol-detection path absorbs the size. With the
      // tokenLooksLikeNonSymbol guard, it's filtered before
      // transformSymbol runs.
      const exchange = createMockExchange();
      exchange.store!.markets = [
        ...exchange.store!.markets!,
        { symbol: '1000USDT', base: '1000', quote: 'USDT', precision: { amount: 1, price: 0.0001 } } as never,
      ];
      exchange.store!.tickers = [
        ...exchange.store!.tickers!,
        { symbol: '1000USDT', last: 0.5 } as never,
      ];
      const ctx = createMockContext(['sell', '1000$'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('BTCUSDT');
      expect(order.side).toBe(OrderSide.Sell);
      expect(order.amount).toBe(0.02);
    });
  });

  describe('multi-symbol fan-out', () => {
    it('places one sell per symbol with `BTCUSDT,ETHUSDT $100`', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['sell', 'BTCUSDT,ETHUSDT', '$100'], exchange);
      const result = await command.execute(ctx);
      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(2);
      const orders = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map(
        (c) => c[0],
      );
      expect(orders.every((o) => o.side === OrderSide.Sell)).toBe(true);
      expect(orders.map((o) => o.symbol).sort()).toEqual(['BTCUSDT', 'ETHUSDT']);
    });

    it('fan-out with `1000$` and a `base="1000"` market still sizes each leg correctly', async () => {
      const exchange = createMockExchange();
      exchange.store!.markets = [
        ...exchange.store!.markets!,
        { symbol: '1000USDT', base: '1000', quote: 'USDT', precision: { amount: 1, price: 0.0001 } } as never,
      ];
      exchange.store!.tickers = [
        ...exchange.store!.tickers!,
        { symbol: '1000USDT', last: 0.5 } as never,
      ];
      const ctx = createMockContext(
        ['sell', 'BTCUSDT,ETHUSDT', '1000$'],
        exchange,
      );
      const result = await command.execute(ctx);
      expect(result.success).toBe(true);
      const orders = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map(
        (c) => c[0],
      );
      expect(orders).toHaveLength(2);
      const bySymbol = Object.fromEntries(orders.map((o) => [o.symbol, o]));
      expect(bySymbol.BTCUSDT.amount).toBeCloseTo(0.02);
      expect(bySymbol.ETHUSDT.amount).toBeCloseTo(0.333, 2);
      expect(orders.every((o) => o.symbol !== '1000USDT')).toBe(true);
    });
  });
});
