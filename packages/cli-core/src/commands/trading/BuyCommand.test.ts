import { describe, it, expect, vi, beforeEach } from 'vitest';
import { BuyCommand } from './BuyCommand';
import type { CommandContext } from '../Command';
import { OrderSide, OrderType, OrderTimeInForce } from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';
import { MemoryPreprocessStore } from '../../preprocess/memoryStore';

// Helper to create mock exchange
function createMockExchange(overrides: Partial<ExchangeLike> = {}): ExchangeLike {
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
  preprocessStore?: MemoryPreprocessStore,
): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol,
    accountName: 'test-account',
    exchange,
    preprocessStore,
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

describe('BuyCommand', () => {
  let command: BuyCommand;

  beforeEach(() => {
    command = new BuyCommand();
  });

  describe('command properties', () => {
    it('has correct name', () => {
      expect(command.name).toBe('buy');
    });

    it('has correct aliases', () => {
      expect(command.aliases).toContain('b');
    });

    it('has correct description', () => {
      expect(command.description).toBe('Buy market or limit order');
    });

    it('requires connection', () => {
      expect(command.requiresConnection).toBe(true);
    });

    it('returns Buy side', () => {
      expect(command['side']).toBe(OrderSide.Buy);
    });
  });

  describe('error handling', () => {
    it('returns error when not connected to exchange', async () => {
      const ctx = createMockContext(['buy', '$100']);
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Not connected to exchange');
    });

    it('returns error when exchange does not support placing orders', async () => {
      const exchange = createMockExchange({ placeOrder: undefined });
      const ctx = createMockContext(['buy', '$100'], exchange);
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Exchange does not support placing orders');
    });

    it('returns error when size argument is missing', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy'], exchange);
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toContain('Size argument missing');
    });
  });

  describe('market orders', () => {
    it('places market buy order with session symbol', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'BTCUSDT',
          side: OrderSide.Buy,
          type: OrderType.Market,
          reduceOnly: false,
          timeInForce: OrderTimeInForce.GoodTillCancel,
        }),
      );
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith('Order placed');
    });

    it('places market buy order with symbol at start', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', 'ETHUSDT', '$300'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'ETHUSDT',
          side: OrderSide.Buy,
          type: OrderType.Market,
        }),
      );
    });

    it('places market buy order with symbol at end', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$300', 'ETHUSDT'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'ETHUSDT',
          side: OrderSide.Buy,
          type: OrderType.Market,
        }),
      );
    });

    it('places reduce-only market buy order', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', 'reduce'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'BTCUSDT',
          side: OrderSide.Buy,
          type: OrderType.Market,
          reduceOnly: true,
        }),
      );
    });

    it('places orders for multiple symbols', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', 'BTCUSDT,ETHUSDT', '$100'], exchange);
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(2);
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith('Orders placed');
    });

    it('detects bare numbers that look like prices (safety check)', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', '50000'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Invalid syntax');
      expect(ctx.adapter.printError).toHaveBeenCalledWith(expect.stringContaining('did you mean'));
    });

    /**
     * The guard used to fire only at `>= 100`, a threshold that assumed
     * BTC-shaped prices. On a coin quoted in cents it never fired, so the
     * price was DROPPED and the order went out at market — reported by a user
     * as "my limit filled instantly at a terrible price". There is no reading
     * of a leftover bare number that means anything, so any magnitude refuses.
     */
    it('refuses a forgotten price on a sub-dollar coin, not just a big one', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '100$', '0.051124'], exchange, 'WIFUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Invalid syntax');
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });

    /**
     * When the symbol LEADS, args[2] is the SIZE slot, and the stray-number
     * scan must not start there. A bare size is still refused — by
     * `parseSizeAndConvertIfUsd`, which says why ("Use $1000, 1000$, or NN%")
     * — and that is the message the user needs. Scanning from index 2 replaced
     * it with "did you mean `at 1`?", which sends someone hunting for a limit
     * price they never meant to type.
     */
    it('refuses a bare size after a leading symbol with the SIZE error, not the price one', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', 'BTCUSDT', '1'], exchange, 'ETHUSDT');
      await expect(command.execute(ctx)).rejects.toThrow(/Invalid size/);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });

    it('places a sized order when the symbol comes first', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', 'BTCUSDT', '$100'], exchange, 'ETHUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const placed = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      expect(placed.length).toBeGreaterThan(0);
      for (const order of placed) expect(order.symbol).toBe('BTCUSDT');
    });

    it('still places a plain market order when no stray number is present', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '100$', 'reduce'], exchange, 'WIFUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalled();
    });
  });

  describe('limit orders', () => {
    /**
     * The market and `best` paths skipped `stop` in their trailing-symbol scan
     * and this one did not, so the walk back from the end hit the literal
     * token `stop` and `resolveMarketSymbol` prefix-matched `STOP` against
     * whatever base a venue happens to list — routing an entire bracket to a
     * market the user never named.
     */
    it('does not read the `stop` keyword as a trailing symbol', async () => {
      const exchange = createMockExchange();
      // `resolveMarketSymbol` PREFIX-matches, so a venue listing anything
      // whose base starts with S is enough for `STOP` to resolve. Bybit lists
      // several; the default fixture happens to list none, which is why this
      // bug survived.
      (exchange.store as { markets: unknown[] }).markets.push({
        symbol: 'SUSDT',
        base: 'S',
        quote: 'USDT',
        precision: { amount: 1, price: 0.0001 },
      });
      // `validateSymbol` checks TICKERS, not markets, so a market alone is not
      // enough to reproduce what a live account does.
      (exchange.store as { tickers: unknown[] }).tickers.push({
        symbol: 'SUSDT',
        last: 0.5,
        bid: 0.49,
        ask: 0.51,
        mark: 0.5,
      });
      const ctx = createMockContext(['buy', '100$', 'at', '49000', 'stop', '48000'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const placed = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      expect(placed.length).toBeGreaterThan(0);
      for (const order of placed) expect(order.symbol).toBe('BTCUSDT');
    });

    /**
     * The other side of the `stop` skip: a REAL trailing symbol must still be
     * found. It is, because the scan walks from the end and reaches the symbol
     * before it ever reaches `stop` — but that is a property worth pinning
     * rather than re-deriving.
     */
    it('still finds a trailing symbol placed after an attached stop', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(
        ['buy', '$100', 'at', '3000', 'stop', '2900', 'ETHUSDT'],
        exchange,
        'BTCUSDT',
      );
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const placed = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      expect(placed.length).toBeGreaterThan(0);
      for (const order of placed) expect(order.symbol).toBe('ETHUSDT');
    });

    it('places limit buy order with "at" keyword', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', 'at', '49000'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'BTCUSDT',
          side: OrderSide.Buy,
          type: OrderType.Limit,
          price: 49000,
          reduceOnly: false,
        }),
      );
    });

    it('places limit buy order with symbol at start', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', 'ETHUSDT', '$300', 'at', '2900'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'ETHUSDT',
          side: OrderSide.Buy,
          type: OrderType.Limit,
          price: 2900,
        }),
      );
    });

    it('places limit buy order with symbol at end', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$300', 'at', '2900', 'ETHUSDT'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'ETHUSDT',
          side: OrderSide.Buy,
          type: OrderType.Limit,
          price: 2900,
        }),
      );
    });

    it('returns error when price argument is missing after "at"', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', 'at'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Price argument missing after "at" keyword');
    });

    it('handles relative price with + prefix', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', 'at', '+100'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          price: 50100, // 50000 + 100
        }),
      );
    });

    it('handles relative price with - prefix', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', 'at', '-500'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          price: 49500, // 50000 - 500
        }),
      );
    });

    it('handles percentage price', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', 'at', '-5%'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          price: 47500, // 50000 - 5%
        }),
      );
    });
  });

  describe('sizing at the order price', () => {
    it('sizes a balance percentage at the limit price', async () => {
      const exchange = createMockExchange();
      exchange.store!.markets![0].precision.amount = 0.0001;
      exchange.store!.tickers![0].last = 82_000;
      exchange.store!.balance = { free: 500, total: 500, used: 0, upnl: 0 };

      const result = await command.execute(createMockContext(['buy', '10%', 'at', '60000'], exchange));

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(expect.objectContaining({ price: 60_000, amount: 0.0008 }));
    });
  });

  describe('size conversion', () => {
    it('converts USD notional to contract size', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$500'], exchange, 'BTCUSDT');
      await command.execute(ctx);

      // $500 / $50000 per BTC = 0.01 BTC, adjusted for precision 0.001
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          amount: 0.01,
        }),
      );
    });

    it('end-to-end: `buy 1000$` (suffix form) places an order with the right amount', async () => {
      // Regression: shipped Phase 0 / Phase 2 added the suffix size
      // form to looksLikeSize but no e2e BuyCommand test verified it.
      // The user reported `buy 1000$` reaching the exchange with no
      // qty — reproduce and lock down the fix.
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '1000$'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('BTCUSDT');
      expect(order.side).toBe(OrderSide.Buy);
      expect(order.type).toBe(OrderType.Market);
      // $1000 / $50000 per BTC = 0.02 BTC, adjusted to 0.001 precision
      expect(order.amount).toBe(0.02);
      expect(order.amount).toBeGreaterThan(0);
    });

    it('end-to-end: `sell 100$` (suffix form) also works', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '100$'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      // $100 / $50000 = 0.002 BTC at 0.001 precision
      expect(order.amount).toBe(0.002);
    });

    it('end-to-end: `buy 1000$ ETHUSDT` with trailing instrument', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '1000$', 'ETHUSDT'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('ETHUSDT');
      // $1000 / $3000 per ETH = 0.333 → adjusted to 0.01 precision = 0.33
      expect(order.amount).toBe(0.33);
      expect(order.amount).toBeGreaterThan(0);
    });

    it('end-to-end: `buy ETHUSDT 1000$` with leading instrument', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', 'ETHUSDT', '1000$'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('ETHUSDT');
      expect(order.amount).toBe(0.33);
    });

    it('end-to-end: `buy 1000$ at 49000` — limit form with suffix size', async () => {
      const exchange = createMockExchange();
      exchange.store!.markets![0].precision.amount = 0.000001;
      const ctx = createMockContext(['buy', '1000$', 'at', '49000'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.type).toBe(OrderType.Limit);
      expect(order.price).toBe(49000);
      expect(order.amount).toBe(0.020408);
    });

    it('end-to-end: `buy 100$` (suffix size, generic case)', async () => {
      // User-reported regression: `buy 100$` should size as $100 →
      // 0.002 BTC at $50k. Locks in that the suffix-USD form works
      // for any leading digit (not just '1000' which hit the
      // tokenLooksLikeNonSymbol path earlier).
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '100$'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);
      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('BTCUSDT');
      // $100 / $50000 = 0.002 BTC
      expect(order.amount).toBeCloseTo(0.002);
    });

    it('end-to-end: `buy 100$` on an INVERSE $1-per-contract market', async () => {
      // User-reported regression (on BitMEX, since retired): inverse contracts
      // are USD-denominated — 1 contract = $1. The previous
      // notionalToSize divided by price ($100 / $50k = 0.002), which
      // BitMEX then adjusted to precision-1 = 0 → "orderQty is
      // required" rejection.
      //
      // With market.isInverse=true, notionalToSize must skip the
      // divide-by-price step and send the notional directly.
      const exchange = createMockExchange();
      exchange.store!.markets = [
        {
          symbol: 'XBTUSD',
          base: 'XBT',
          quote: 'USD',
          precision: { amount: 1, price: 0.5 },
          isInverse: true,
        } as never,
      ];
      exchange.store!.tickers = [{ symbol: 'XBTUSD', last: 50_000 } as never];
      const ctx = createMockContext(['buy', '100$'], exchange, 'XBTUSD');
      const result = await command.execute(ctx);
      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('XBTUSD');
      // Inverse: 100 contracts (= $100 USD), NOT 0.002.
      expect(order.amount).toBe(100);
    });

    it('end-to-end: `buy 1000$` on inverse still skips divide-by-price', async () => {
      const exchange = createMockExchange();
      exchange.store!.markets = [
        {
          symbol: 'XBTUSD',
          base: 'XBT',
          quote: 'USD',
          precision: { amount: 1, price: 0.5 },
          isInverse: true,
        } as never,
      ];
      exchange.store!.tickers = [{ symbol: 'XBTUSD', last: 50_000 } as never];
      const ctx = createMockContext(['buy', '1000$'], exchange, 'XBTUSD');
      await command.execute(ctx);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.amount).toBe(1000);
    });

    it('end-to-end: inverse with `contractSize=100` (e.g. Binance Coin-M BTCUSD_PERP)', async () => {
      // Hypothetical exchange where each inverse contract is $100
      // notional, not $1. The formula must be `notional / contractSize`,
      // not the naive `notional = contracts` shortcut that worked
      // accidentally for Bybit/Bitget (contractSize = 1).
      const exchange = createMockExchange();
      exchange.store!.markets = [
        {
          symbol: 'BTCUSD_PERP',
          base: 'BTC',
          quote: 'USD',
          precision: { amount: 1, price: 0.1 },
          isInverse: true,
          contractSize: 100,
        } as never,
      ];
      exchange.store!.tickers = [{ symbol: 'BTCUSD_PERP', last: 50_000 } as never];
      const ctx = createMockContext(['buy', '1000$'], exchange, 'BTCUSD_PERP');
      await command.execute(ctx);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      // $1000 notional / $100 per contract = 10 contracts.
      expect(order.amount).toBe(10);
    });

    it('end-to-end: LINEAR with `contractSize=1000` (OKX DOGE-USDT-SWAP)', async () => {
      // Linear is not always 1 contract = 1 base coin. OKX publishes a real
      // ctVal on its linear swaps, so the base amount must still be divided by
      // contractSize. Sending the raw base amount made a $10 order a $10,000
      // one, which OKX rejected as "Insufficient USDT margin (51008)".
      const exchange = createMockExchange();
      exchange.store!.markets = [
        {
          symbol: 'DOGEUSDT',
          base: 'DOGE',
          quote: 'USDT',
          precision: { amount: 0.01, price: 0.00001 },
          isInverse: false,
          contractSize: 1000,
        } as never,
      ];
      exchange.store!.tickers = [{ symbol: 'DOGEUSDT', last: 0.07 } as never];
      const ctx = createMockContext(['buy', '10$'], exchange, 'DOGEUSDT');
      await command.execute(ctx);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      // $10 / $0.07 = 142.857 DOGE, / 1000 DOGE per contract = 0.142857,
      // adjusted to the 0.01 lot size = 0.14 contracts (~$9.80 notional).
      expect(order.amount).toBeCloseTo(0.14, 5);
    });

    it('end-to-end: LINEAR with `contractSize=0.01` (OKX BTC-USDT-SWAP)', async () => {
      // The same bug in the other direction: a ctVal below 1 makes the base
      // amount smaller than the lot size, so it adjusts to zero and the order
      // is rejected as "must be finite and positive after adjustment".
      const exchange = createMockExchange();
      exchange.store!.markets = [
        {
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          precision: { amount: 0.01, price: 0.1 },
          isInverse: false,
          contractSize: 0.01,
        } as never,
      ];
      exchange.store!.tickers = [{ symbol: 'BTCUSDT', last: 65_000 } as never];
      const ctx = createMockContext(['buy', '650$'], exchange, 'BTCUSDT');
      await command.execute(ctx);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      // $650 / $65,000 = 0.01 BTC, / 0.01 BTC per contract = 1 contract.
      expect(order.amount).toBeCloseTo(1, 5);
    });

    it('end-to-end: `buy 10%bal` on inverse uses notional-as-contracts', async () => {
      const exchange = createMockExchange();
      exchange.store!.markets = [
        {
          symbol: 'XBTUSD',
          base: 'XBT',
          quote: 'USD',
          precision: { amount: 1, price: 0.5 },
          isInverse: true,
        } as never,
      ];
      exchange.store!.tickers = [{ symbol: 'XBTUSD', last: 50_000 } as never];
      exchange.store!.balance = {
        free: 10_000,
        total: 10_000,
        used: 0,
        upnl: 0,
      } as never;
      // 10% of $10_000 free * 1x leverage = $1_000 notional → 1000 contracts
      const ctx = createMockContext(['buy', '10%bal'], exchange, 'XBTUSD');
      const result = await command.execute(ctx);
      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.amount).toBe(1000);
    });

    it('end-to-end: `buy 1000$ best` — best-quote form with suffix size', async () => {
      const exchange = createMockExchange();
      // Best needs bid/ask on the ticker.
      exchange.store!.tickers = [{ symbol: 'BTCUSDT', last: 50000, bid: 49995, ask: 50005 } as never];
      const ctx = createMockContext(['buy', '1000$', 'best'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.type).toBe(OrderType.Limit);
      expect(order.price).toBe(49995);
      expect(order.amount).toBeGreaterThan(0);
    });

    it('e2e: `buy 1000$` is NOT misidentified as a 1000PEPEUSDT-style symbol', async () => {
      // Regression coverage for transformSymbol's fuzzy base-prefix
      // matching. With a market like 1000PEPEUSDT (base "1000PEPE"),
      // `'1000$'.startsWith('1000PEPE')` is false (different chars
      // at index 4) so the symbol-detection path skips it and we
      // fall through to "size on focused symbol".
      const exchange = createMockExchange();
      exchange.store!.markets = [
        ...exchange.store!.markets!,
        { symbol: '1000PEPEUSDT', base: '1000PEPE', quote: 'USDT', precision: { amount: 1, price: 0.000001 } } as never,
      ];
      exchange.store!.tickers = [...exchange.store!.tickers!, { symbol: '1000PEPEUSDT', last: 0.001 } as never];
      const ctx = createMockContext(['buy', '1000$'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      // Should still buy on the focused symbol (BTC), not 1000PEPEUSDT.
      expect(order.symbol).toBe('BTCUSDT');
      expect(order.amount).toBe(0.02);
    });

    it('e2e: `buy 1000$` when an exchange has a base="1000" market — what happens?', async () => {
      // Some exchanges list a "1000-something" coin where the BASE is
      // literally "1000". `'1000$'.startsWith('1000')` is true, so
      // transformSymbol's fuzzy fallback finds a base match. This is
      // the actual cause of `buy 1000$` getting routed as a SYMBOL
      // instead of a SIZE on those exchanges.
      const exchange = createMockExchange();
      exchange.store!.markets = [
        ...exchange.store!.markets!,
        { symbol: '1000USDT', base: '1000', quote: 'USDT', precision: { amount: 1, price: 0.0001 } } as never,
      ];
      exchange.store!.tickers = [...exchange.store!.tickers!, { symbol: '1000USDT', last: 0.5 } as never];
      const ctx = createMockContext(['buy', '1000$'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      // Without the fix, this errors with "Size argument missing"
      // (because '1000$' was eaten as the symbol and no size remains).
      // With the fix, '1000$' must always be treated as a size, never
      // as a symbol — even when the base-prefix happens to match.
      expect(result.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('BTCUSDT');
      expect(order.amount).toBe(0.02);
    });
  });

  describe('multi-symbol fan-out', () => {
    it('places one order per symbol with `BTCUSDT,ETHUSDT $100`', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', 'BTCUSDT,ETHUSDT', '$100'], exchange);
      const result = await command.execute(ctx);
      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(2);
      const calls = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0].symbol);
      expect(calls.sort()).toEqual(['BTCUSDT', 'ETHUSDT']);
    });

    it('fan-out with limit price applies the same price to every leg', async () => {
      const exchange = createMockExchange();
      // $1000 so the ETH leg, sized at 49000, still clears its 0.01 lot step.
      const ctx = createMockContext(['buy', 'BTCUSDT,ETHUSDT', '$1000', 'at', '49000'], exchange);
      const result = await command.execute(ctx);
      expect(result.success).toBe(true);
      const orders = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      expect(orders).toHaveLength(2);
      expect(orders.every((o) => o.type === OrderType.Limit)).toBe(true);
      expect(orders.every((o) => o.price === 49000)).toBe(true);
    });

    it('fan-out with `1000$` and a `base="1000"` market still sizes each leg correctly', async () => {
      // This is the multi-symbol version of the `buy 1000$` regression
      // — comma-separated symbols must not let `'1000$'` be claimed as
      // a third symbol via fuzzy base matching.
      const exchange = createMockExchange();
      exchange.store!.markets = [
        ...exchange.store!.markets!,
        { symbol: '1000USDT', base: '1000', quote: 'USDT', precision: { amount: 1, price: 0.0001 } } as never,
      ];
      exchange.store!.tickers = [...exchange.store!.tickers!, { symbol: '1000USDT', last: 0.5 } as never];
      const ctx = createMockContext(['buy', 'BTCUSDT,ETHUSDT', '1000$'], exchange);
      const result = await command.execute(ctx);
      expect(result.success).toBe(true);
      const orders = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      expect(orders).toHaveLength(2);
      const bySymbol = Object.fromEntries(orders.map((o) => [o.symbol, o]));
      // $1000 / 50000 = 0.02 BTC; $1000 / 3000 ≈ 0.333 ETH
      expect(bySymbol.BTCUSDT.amount).toBeCloseTo(0.02);
      expect(bySymbol.ETHUSDT.amount).toBeCloseTo(0.333, 2);
      expect(orders.every((o) => o.symbol !== '1000USDT')).toBe(true);
    });

    it('fan-out with percent size sizes each leg against its own ticker', async () => {
      const exchange = createMockExchange();
      // Free balance is 1_000_000 in createMockExchange... actually it's
      // not set. `%bal` needs balance — set one explicitly.
      exchange.store!.balance = { used: 0, free: 200_000, total: 200_000, upnl: 0 } as never;
      const ctx = createMockContext(['buy', 'BTCUSDT,ETHUSDT', '1%bal'], exchange);
      const result = await command.execute(ctx);
      expect(result.success).toBe(true);
      const orders = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      expect(orders).toHaveLength(2);
      const bySymbol = Object.fromEntries(orders.map((o) => [o.symbol, o]));
      // 1% of 200_000 = $2_000. BTC@50_000 → 0.04, ETH@3_000 → ~0.667
      expect(bySymbol.BTCUSDT.amount).toBeCloseTo(0.04);
      expect(bySymbol.ETHUSDT.amount).toBeCloseTo(0.667, 2);
    });
  });

  describe('fatfinger', () => {
    it('rejects an order whose computed amount exceeds the limit', async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      store.setFatfinger('BTCUSDT', 0.01); // 0.01 BTC max
      // $1000 / 50_000 = 0.02 → exceeds 0.01 limit.
      const ctx = createMockContext(['buy', '$1000'], exchange, 'BTCUSDT', store);
      await expect(command.execute(ctx)).rejects.toThrow(/fatfinger.*BTCUSDT.*0.02.*limit 0.01/);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });

    it('allows an order at or below the limit', async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      store.setFatfinger('BTCUSDT', 0.05);
      // $1000 / 50_000 = 0.02 → within 0.05 limit.
      const ctx = createMockContext(['buy', '$1000'], exchange, 'BTCUSDT', store);
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
    });

    it("only checks the matching symbol — limits on other symbols don't bleed", async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      store.setFatfinger('ETHUSDT', 0.001); // tight limit on a different symbol
      const ctx = createMockContext(['buy', '$5000'], exchange, 'BTCUSDT', store);
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
    });

    it('skips fatfinger on reduce-only orders (closing exposure is fine)', async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      store.setFatfinger('BTCUSDT', 0.01);
      // reduceOnly + amount above limit → still goes through
      const ctx = createMockContext(['buy', '$1000', 'reduce'], exchange, 'BTCUSDT', store);
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
    });

    it('limit orders are gated too', async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      store.setFatfinger('BTCUSDT', 0.01);
      const ctx = createMockContext(['buy', '$1000', 'at', '49000'], exchange, 'BTCUSDT', store);
      await expect(command.execute(ctx)).rejects.toThrow(/fatfinger/);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });
  });

  describe('simulation', () => {
    it('does not call placeOrder when simulation is on', async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      store.setSimulation(true);
      const ctx = createMockContext(['buy', '$1000'], exchange, 'BTCUSDT', store);
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });

    it('logs a [sim] line with the original command', async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      store.setSimulation(true);
      const ctx = createMockContext(['buy', '$1000', 'at', '49000'], exchange, 'BTCUSDT', store);
      await command.execute(ctx);
      expect(ctx.adapter.printInfo).toHaveBeenCalledWith('[sim] buy $1000 at 49000');
    });

    it('lets live trading resume after `simulation off`', async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      store.setSimulation(true);
      const ctxA = createMockContext(['buy', '$1000'], exchange, 'BTCUSDT', store);
      await command.execute(ctxA);
      expect(exchange.placeOrder).not.toHaveBeenCalled();

      store.setSimulation(false);
      const ctxB = createMockContext(['buy', '$1000'], exchange, 'BTCUSDT', store);
      await command.execute(ctxB);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
    });
  });

  describe('po keyword (post-only) — limit orders', () => {
    it('`buy $100 at 49000 po` sets timeInForce: PostOnly', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', 'at', '49000', 'po'], exchange, 'BTCUSDT');
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.type).toBe(OrderType.Limit);
      expect(order.timeInForce).toBe(OrderTimeInForce.PostOnly);
    });

    it('without `po`, limit orders still use GoodTillCancel', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', 'at', '49000'], exchange, 'BTCUSDT');
      await command.execute(ctx);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.timeInForce).toBe(OrderTimeInForce.GoodTillCancel);
    });

    it('`po` on a market order is silently ignored (no timeInForce flip)', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', 'po'], exchange, 'BTCUSDT');
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.type).toBe(OrderType.Market);
      // Market orders default to GoodTillCancel; the `po` keyword
      // doesn't flip it.
      expect(order.timeInForce).toBe(OrderTimeInForce.GoodTillCancel);
    });

    it('`po` combines with `reduce`', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', 'at', '49000', 'reduce', 'po'], exchange, 'BTCUSDT');
      await command.execute(ctx);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.reduceOnly).toBe(true);
      expect(order.timeInForce).toBe(OrderTimeInForce.PostOnly);
    });

    it('`po` placed before a trailing symbol still parses correctly', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', 'at', '2900', 'po', 'ETHUSDT'], exchange, 'BTCUSDT');
      await command.execute(ctx);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('ETHUSDT');
      expect(order.timeInForce).toBe(OrderTimeInForce.PostOnly);
    });

    it('`po` works on the `best` form', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['buy', '$100', 'best', 'po'], exchange, 'BTCUSDT');
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.type).toBe(OrderType.Limit);
      expect(order.timeInForce).toBe(OrderTimeInForce.PostOnly);
    });
  });

  describe('result.data.orderIds — capture-syntax support', () => {
    // The runner reads `data.orderIds` and writes the comma-joined
    // list into the named user var when the user prefixed the
    // command with `<var> =`. These tests lock the data shape in
    // for every code path that places an order.

    it('market order surfaces the exchange-returned ID', async () => {
      const exchange = createMockExchange();
      (exchange.placeOrder as ReturnType<typeof vi.fn>).mockResolvedValue(['mkt-abc']);
      const r = await command.execute(createMockContext(['buy', '$100'], exchange));
      expect(r.success).toBe(true);
      expect(r.data?.orderIds).toEqual(['mkt-abc']);
    });

    it('limit order surfaces the exchange-returned ID', async () => {
      const exchange = createMockExchange();
      (exchange.placeOrder as ReturnType<typeof vi.fn>).mockResolvedValue(['lim-xyz']);
      const r = await command.execute(createMockContext(['buy', '$100', 'at', '49000'], exchange));
      expect(r.success).toBe(true);
      expect(r.data?.orderIds).toEqual(['lim-xyz']);
    });

    it('`best` form surfaces the ID', async () => {
      const exchange = createMockExchange();
      (exchange.placeOrder as ReturnType<typeof vi.fn>).mockResolvedValue(['best-1']);
      const r = await command.execute(createMockContext(['buy', '$100', 'best'], exchange));
      expect(r.success).toBe(true);
      expect(r.data?.orderIds).toEqual(['best-1']);
    });

    it('bracket order (entry + SL) surfaces ALL returned IDs', async () => {
      // Exchanges that place SL as a separate order return >1 ID
      // per placeOrder call. The full array makes it back to the
      // caller untouched.
      const exchange = createMockExchange();
      (exchange.placeOrder as ReturnType<typeof vi.fn>).mockResolvedValue(['entry-1', 'sl-2']);
      const r = await command.execute(createMockContext(['buy', '$100', 'at', '49000', 'stop', '48000'], exchange));
      expect(r.success).toBe(true);
      expect(r.data?.orderIds).toEqual(['entry-1', 'sl-2']);
    });

    it('multi-symbol fan-out concatenates IDs across legs in placement order', async () => {
      const exchange = createMockExchange();
      const placeOrder = exchange.placeOrder as ReturnType<typeof vi.fn>;
      placeOrder.mockResolvedValueOnce(['btc-1']).mockResolvedValueOnce(['eth-1']);
      const r = await command.execute(createMockContext(['buy', 'BTCUSDT,ETHUSDT', '$100'], exchange));
      expect(r.success).toBe(true);
      expect(r.data?.orderIds).toEqual(['btc-1', 'eth-1']);
    });

    it('empty-array return (e.g. Bybit position-stop endpoint) surfaces as []', async () => {
      // Bybit's SET_TRADING_STOP path returns [] because position
      // stops aren't discrete orders. The capture path should
      // preserve the empty array; the runner is responsible for
      // warning the user.
      const exchange = createMockExchange();
      (exchange.placeOrder as ReturnType<typeof vi.fn>).mockResolvedValue([]);
      const r = await command.execute(createMockContext(['buy', '$100'], exchange));
      expect(r.success).toBe(true);
      expect(r.data?.orderIds).toEqual([]);
    });

    it('a placeOrder returning null/undefined is coerced to []', async () => {
      // Defensive — older or partially-implemented exchange adapters
      // might omit the array. Capture path must not crash and must
      // surface the same "empty" signal the runner already knows
      // how to warn about.
      const exchange = createMockExchange();
      (exchange.placeOrder as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
      const r = await command.execute(createMockContext(['buy', '$100'], exchange));
      expect(r.success).toBe(true);
      expect(r.data?.orderIds).toEqual([]);
    });
  });

  // TSBUG-102: a typo'd explicit symbol must NOT silently route the
  // order to the focused symbol. It has to fail closed.
  describe('unknown-symbol guard', () => {
    it('errors on a trailing unknown symbol instead of filling the focused symbol', async () => {
      const exchange = createMockExchange();
      const r = await command.execute(createMockContext(['buy', '$100', 'XBTUSD'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/Unknown symbol "XBTUSD"/);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });

    it('errors on a leading unknown symbol', async () => {
      const exchange = createMockExchange();
      const r = await command.execute(createMockContext(['buy', 'XBTUSD', '$100'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/Unknown symbol "XBTUSD"/);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });

    it('errors on an unknown symbol in a limit order', async () => {
      const exchange = createMockExchange();
      const r = await command.execute(createMockContext(['buy', '$100', 'at', '49000', 'XBTUSD'], exchange));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/Unknown symbol "XBTUSD"/);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });

    it('still falls back to the focused symbol when no symbol is given', async () => {
      const exchange = createMockExchange();
      const r = await command.execute(createMockContext(['buy', '$100'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(expect.objectContaining({ symbol: 'BTCUSDT' }));
    });

    it('does not flag valid symbols, sizes, prices, or keywords', async () => {
      const exchange = createMockExchange();
      const r = await command.execute(createMockContext(['buy', 'ETHUSDT', '$100', 'at', '2900', 'reduce'], exchange));
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(expect.objectContaining({ symbol: 'ETHUSDT' }));
    });

    it('fails closed on a comma list with an invalid leg (no partial routing)', async () => {
      const exchange = createMockExchange();
      const r = await command.execute(createMockContext(['buy', 'BTCUSDT,XBTUSD', '$100'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/Unknown symbol "XBTUSD"/);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });

    it('accepts a comma list when every leg resolves', async () => {
      const exchange = createMockExchange();
      const r = await command.execute(createMockContext(['buy', 'BTCUSDT,ETHUSDT', '$100'], exchange));
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(2);
    });
  });

  // TSBUG-105: a bare `%` on a reduce-only order must size from the open
  // POSITION (you're closing), not free balance. The mock has no balance,
  // so a balance-scope resolution would throw — position scope succeeds.
  describe('reduce-only percent sizing', () => {
    it('sizes a bare `%` reduce order from the position, not balance', async () => {
      const exchange = createMockExchange();
      // Short position of 4 contracts; `buy 50% reduce` closes half of it.
      (exchange.store as { positions: unknown[] }).positions = [
        { symbol: 'BTCUSDT', side: 'short', contracts: -4, leverage: 10 },
      ];
      const r = await command.execute(createMockContext(['buy', '50%', 'reduce'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(expect.objectContaining({ amount: 2, reduceOnly: true }));
    });

    it('a non-reduce bare `%` still uses balance scope (errors when no balance)', async () => {
      const exchange = createMockExchange();
      (exchange.store as { positions: unknown[] }).positions = [
        { symbol: 'BTCUSDT', side: 'short', contracts: -4, leverage: 10 },
      ];
      // No balance on the mock → balance scope can't resolve. The helper
      // throws; CommandExecutor converts that to success:false in prod.
      await expect(command.execute(createMockContext(['buy', '50%'], exchange, 'BTCUSDT'))).rejects.toThrow(
        /Balance not available/,
      );
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });
  });

  // TSBUG-106: opening/increasing exposure is blocked when the private
  // data stream is degraded; reduce-only orders (risk-reducing) are not.
  describe('health gate', () => {
    const degrade = (exchange: ExchangeLike) => {
      exchange.store!.socketHealth = {
        publicHealthy: true,
        privateHealthy: false,
        publicInitialized: true,
        privateInitialized: true,
      };
    };

    it('blocks an opening buy when the private stream is degraded', async () => {
      const exchange = createMockExchange();
      degrade(exchange);
      // Gated at placement → throws; CommandExecutor surfaces it as
      // success:false in prod. No order reaches the exchange.
      await expect(command.execute(createMockContext(['buy', '$100'], exchange, 'BTCUSDT'))).rejects.toThrow(
        /degraded/i,
      );
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });

    it('allows a reduce-only order even when degraded', async () => {
      const exchange = createMockExchange();
      degrade(exchange);
      const r = await command.execute(createMockContext(['buy', '$100', 'reduce'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(expect.objectContaining({ reduceOnly: true }));
    });

    it('allows an opening buy when healthy', async () => {
      const exchange = createMockExchange();
      exchange.store!.socketHealth = {
        publicHealthy: true,
        privateHealthy: true,
        publicInitialized: true,
        privateInitialized: true,
      };
      const r = await command.execute(createMockContext(['buy', '$100'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalled();
    });

    it('fails open when the private socket is not initialized', async () => {
      const exchange = createMockExchange();
      exchange.store!.socketHealth = {
        publicHealthy: true,
        privateHealthy: false,
        publicInitialized: true,
        privateInitialized: false,
      };
      const r = await command.execute(createMockContext(['buy', '$100'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalled();
    });

    it('surfaces the parse error (not the degraded message) for a malformed command', async () => {
      const exchange = createMockExchange();
      degrade(exchange);
      // `buy` with no size — should report the size/parse problem, and
      // must not reach placeOrder. (The gate now sits at placement, so a
      // command that never places isn't masked by the degraded message.)
      const r = await command.execute(createMockContext(['buy'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(false);
      expect(r.error).not.toMatch(/degraded/i);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });
  });

  // TSBUG-106: opt-in aggregate notional budget caps cumulative
  // exposure-increasing order notional this session.
  describe('aggregate budget', () => {
    it('is off by default (no budget set → no gating)', async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      const r = await command.execute(createMockContext(['buy', '$1000000'], exchange, 'BTCUSDT', store));
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalled();
    });

    it('allows an order within the cap and charges it', async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      store.setBudget(100); // $100 cap; BTCUSDT @ 50000, $100 notional = exactly 100
      const r = await command.execute(createMockContext(['buy', '$100'], exchange, 'BTCUSDT', store));
      expect(r.success).toBe(true);
      expect(store.getBudgetSpent()).toBeCloseTo(100);
    });

    it('blocks an order that would exceed the cap', async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      store.setBudget(100);
      await expect(command.execute(createMockContext(['buy', '$150'], exchange, 'BTCUSDT', store))).rejects.toThrow(
        /budget/i,
      );
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });

    it('accumulates across orders in the same session', async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      store.setBudget(150); // $100 notional = 0.002 BTC @ 50000 exactly (no rounding surprise)
      const r1 = await command.execute(createMockContext(['buy', '$100'], exchange, 'BTCUSDT', store));
      expect(r1.success).toBe(true);
      expect(store.getBudgetSpent()).toBeCloseTo(100);
      // 100 spent + 100 = 200 > 150 → blocked.
      await expect(command.execute(createMockContext(['buy', '$100'], exchange, 'BTCUSDT', store))).rejects.toThrow(
        /budget/i,
      );
    });

    it('does not count reduce-only orders against the budget', async () => {
      const exchange = createMockExchange();
      const store = new MemoryPreprocessStore();
      store.setBudget(100);
      const r = await command.execute(createMockContext(['buy', '$1000', 'reduce'], exchange, 'BTCUSDT', store));
      expect(r.success).toBe(true);
      expect(store.getBudgetSpent()).toBe(0);
    });
  });

  // TSBUG-105: a size that rounds to 0 at the market's amount precision
  // must not reach the exchange as an empty order.
  describe('zero-size guard', () => {
    it('refuses a $ notional that rounds to 0 contracts', async () => {
      const exchange = createMockExchange();
      // $0.01 / 50000 ≈ 2e-7, adjusted to 0.001 precision → 0. The helper
      // throws; CommandExecutor surfaces it as success:false in prod.
      await expect(command.execute(createMockContext(['buy', '$0.01'], exchange, 'BTCUSDT'))).rejects.toThrow(
        /rounds to 0/,
      );
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });
  });
});
