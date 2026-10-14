import type { ExchangeLike } from '../types';

import { describe, expect, it } from 'vitest';

import { OrderSide, OrderType } from '@tealstreet/safe-cex/src/types';
import { nativeCloseSize, prepareCliAmendPrice, prepareCliOrder } from './prepareCliOrder';

/**
 * These pin what the CLI did NOT do before this file existed. Every one of
 * them is a number that used to reach the venue as typed.
 */

const exchange = (over: Partial<ExchangeLike> = {}): ExchangeLike =>
  ({
    name: 'binance',
    store: {
      markets: [{ symbol: 'BTCUSDT', precision: { amount: 0.001, price: 0.5 }, isSpot: false }],
      tickers: [{ symbol: 'BTCUSDT', last: 60_000 }],
      orders: [],
      positions: [],
    },
    ...over,
  }) as unknown as ExchangeLike;

describe('prepareCliOrder', () => {
  it('rounds a limit price to the venue tick — `buy ... at` never did', () => {
    const out = prepareCliOrder(exchange(), {
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      amount: 1,
      price: 60_000.27,
    });
    expect(out.price).toBe(60_000.5);
  });

  it('rounds a size to the lot step', () => {
    const out = prepareCliOrder(exchange(), {
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Market,
      amount: 1.23456,
    });
    expect(out.amount).toBe(1.235);
  });

  it('refuses a size that rounds below the lot step instead of sending it', () => {
    expect(() =>
      prepareCliOrder(exchange(), {
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Market,
        amount: 0.0004,
      }),
    ).toThrow(/below the venue lot step/);
  });

  it('keeps a reduce-only market close that rounds to zero, so dust can still close', () => {
    const out = prepareCliOrder(exchange(), {
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.Market,
      amount: 0.0004,
      reduceOnly: true,
    });
    expect(out.amount).toBe(0.0004);
  });

  it('rounds a standalone stop trigger — `stop` sent it raw', () => {
    const out = prepareCliOrder(exchange(), {
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      amount: 1,
      triggerPrice: 58_000.31,
      reduceOnly: true,
    });
    expect(out.triggerPrice).toBe(58_000.5);
  });

  it('lets a zero-size infinity stop through where the venue supports one', () => {
    const out = prepareCliOrder(exchange(), {
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      amount: 0,
      triggerPrice: 58_000,
      reduceOnly: true,
    });
    expect(out.amount).toBe(0);
  });

  it('refuses the same zero-size stop on a venue without infinity stops', () => {
    // `stop <price>` with no size sends `amount: 0`. On the twelve venues that
    // do not read that as Full, it is an order for nothing — previously sent
    // anyway and rejected by the venue.
    expect(() =>
      prepareCliOrder(exchange({ name: 'paradex' } as Partial<ExchangeLike>), {
        symbol: 'BTCUSDT',
        side: OrderSide.Sell,
        type: OrderType.StopLoss,
        amount: 0,
        triggerPrice: 58_000,
        reduceOnly: true,
      }),
    ).toThrow(/does not support Full \(Infinity\) TP\/SL/);
  });

  it('refuses rather than sending unrounded numbers when the market is not loaded', () => {
    expect(() =>
      prepareCliOrder(exchange({ store: { markets: [] } } as unknown as Partial<ExchangeLike>), {
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Market,
        amount: 1,
      }),
    ).toThrow(/Market not found/);
  });

  it('does not throw on an unrecognised venue — the clamp gate must not fail the write', () => {
    const out = prepareCliOrder(exchange({ name: 'not-a-venue' } as Partial<ExchangeLike>), {
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.Market,
      amount: 1,
      reduceOnly: true,
    });
    expect(out.amount).toBe(1);
  });

  describe('tradfi preflight', () => {
    // Bybit publishes `symbolType` on `/v5/market/instruments-info` and
    // `fetchMarketsHelper` preserves it as `Market.info`; `classifyMarket`
    // reads from there with no new REST call.
    const bybitEquity = (over: Partial<ExchangeLike> = {}): ExchangeLike =>
      ({
        name: 'bybit',
        store: {
          markets: [
            {
              symbol: 'SHOPUSDT',
              precision: { amount: 0.001, price: 0.01 },
              isSpot: false,
              info: { symbolType: 'stock', underlyingTicker: 'SHOP', marketRegion: 'US' },
            },
          ],
          tickers: [{ symbol: 'SHOPUSDT', last: 100 }],
          orders: [],
          positions: [],
        },
        ...over,
      }) as unknown as ExchangeLike;

    const bybitCrypto = (): ExchangeLike =>
      ({
        name: 'bybit',
        store: {
          markets: [
            {
              symbol: 'BTCUSDT',
              precision: { amount: 0.001, price: 0.5 },
              isSpot: false,
              info: { symbolType: '' },
            },
          ],
          tickers: [{ symbol: 'BTCUSDT', last: 60_000 }],
          orders: [],
          positions: [],
        },
      }) as unknown as ExchangeLike;

    it('refuses a buy on a tokenized equity with the "may require" phrasing', () => {
      expect(() =>
        prepareCliOrder(bybitEquity(), {
          symbol: 'SHOPUSDT',
          side: OrderSide.Buy,
          type: OrderType.Market,
          amount: 1,
        }),
      ).toThrow(/tokenized equity.*may require an agreement you have not signed/);
    });

    it('refuses even a reduce-only close — the venue may still reject it on a signed-out account', () => {
      expect(() =>
        prepareCliOrder(bybitEquity(), {
          symbol: 'SHOPUSDT',
          side: OrderSide.Sell,
          type: OrderType.Market,
          amount: 1,
          reduceOnly: true,
        }),
      ).toThrow(/tokenized equity.*may require an agreement/);
    });

    it('lets a crypto perp through on the same venue', () => {
      const out = prepareCliOrder(bybitCrypto(), {
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Market,
        amount: 1,
      });
      expect(out.amount).toBe(1);
    });

    it('never says "you can trade this" or "you cannot trade this"', () => {
      try {
        prepareCliOrder(bybitEquity(), {
          symbol: 'SHOPUSDT',
          side: OrderSide.Buy,
          type: OrderType.Market,
          amount: 1,
        });
        throw new Error('should have thrown');
      } catch (e) {
        const msg = (e as Error).message.toLowerCase();
        expect(msg).not.toContain('you can trade');
        expect(msg).not.toContain('you cannot trade');
      }
    });

    it('lets an unknown classification through — a venue with no classifier is a normal state, not a reason to block', () => {
      // binance has no classifier yet. Pass through, matching the existing
      // default-fixture tests above.
      const out = prepareCliOrder(exchange(), {
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Market,
        amount: 1,
      });
      expect(out.amount).toBe(1);
    });
  });
});

describe('prepareCliAmendPrice', () => {
  it('rounds an amend price to the tick — `bump` and `move` sent it raw', () => {
    expect(prepareCliAmendPrice(exchange(), 'BTCUSDT', 60_000.27)).toBe(60_000.5);
  });

  it('refuses an amend on a symbol with no market', () => {
    expect(() => prepareCliAmendPrice(exchange(), 'ETHUSDT', 3000)).toThrow(/Market not found/);
  });

  it('refuses a price that COLLAPSES to zero when rounded', () => {
    // `bump` and `move` validate the shifted price before handing it over, so
    // a pre-rounding check does not cover this: an order at 0.001 bumped by
    // -0.0008 is a valid 0.0002, and `roundOrderPrice(0.0002, tick 0.001)` is
    // 0. Without this the amend reached the venue as `price: 0`.
    const penny = exchange({
      store: {
        markets: [{ symbol: 'PENNYUSDT', precision: { amount: 1, price: 0.001 }, isSpot: false }],
        tickers: [],
        orders: [],
        positions: [],
      },
    } as never);
    expect(() => prepareCliAmendPrice(penny, 'PENNYUSDT', 0.0002)).toThrow(/rounds to 0/);
  });
});

describe('nativeCloseSize', () => {
  /**
   * The gap an adversarial review found: `cliOrdersGoThroughPolicy.test.ts`
   * proves every command calls `prepareCliOrder`, but `prepareCliOrder` only
   * rounds the amount it is GIVEN. It cannot tell that the number arrived in
   * the wrong unit — and `close` and `%pos` both read `position.contracts`,
   * which is base, straight into a native `amount`.
   */
  const withMarket = (over: Record<string, unknown>) =>
    ({
      name: 'okx',
      store: { markets: [{ symbol: 'DOGEUSDT', precision: { amount: 1, price: 0.00001 }, ...over }] },
    }) as unknown as ExchangeLike;

  it('divides a linear position by contractSize', () => {
    // OKX DOGE-USDT-SWAP: 1000 DOGE per contract. A full close of 5000 DOGE is
    // FIVE contracts; sending 5000 asks for a thousand times the position.
    expect(
      nativeCloseSize(withMarket({ contractSize: 1000, isInverse: false }), {
        symbol: 'DOGEUSDT',
        contracts: 5000,
      }),
    ).toBe(5);
  });

  it('sizes an inverse position from its notional', () => {
    expect(
      nativeCloseSize(withMarket({ contractSize: 100, isInverse: true }), {
        symbol: 'DOGEUSDT',
        contracts: 40,
        notional: 40,
      }),
    ).toBe(0.4);
  });

  it('ignores a stale venue-supplied native size that is smaller', () => {
    expect(
      nativeCloseSize(withMarket({ contractSize: 100, isInverse: true }), {
        symbol: 'DOGEUSDT',
        contracts: 40,
        notional: 40,
        extra: { contractsNative: 0.1 },
      }),
    ).toBe(0.4);
  });

  it('REFUSES when the market has not loaded, rather than falling back to base', () => {
    // The fallback was tempting — it is the unit the code used before, and
    // `prepareCliOrder` refuses a moment later anyway. But nothing awaits
    // between the two, so a market missing here is missing there; the fallback
    // would only make the old bug a tested behaviour and leave a future caller
    // one missed `prepareCliOrder` away from re-opening it.
    const noMarket = { name: 'okx', store: { markets: [] } } as unknown as ExchangeLike;
    expect(() => nativeCloseSize(noMarket, { symbol: 'DOGEUSDT', contracts: 5000 })).toThrow(/Market not found/);
  });
});
