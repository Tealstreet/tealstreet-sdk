import type { OemsContext } from './context.types';
import type { OemsIntent, OemsPlan } from './intent.types';

import type { OemsOverride } from './resolvePlan';

import { resolveOemsPlan } from './resolvePlan';

const NOW = 1_700_000_000_000;

/**
 * One fixture book carrying the three cases that break naive arithmetic: a
 * hedged pair on one symbol, an inverse contract, and a spot market.
 */
const context = (over: Partial<OemsContext> = {}): OemsContext => ({
  generatedAt: NOW,
  activeAccount: 'binance-main',
  activeSymbol: 'BTCUSDT',
  activeAccounts: ['binance-main', 'bybit-hedge'],
  accounts: [
    { name: 'binance-main', exchange: 'binance', active: true, testnet: false, connected: true },
    { name: 'bybit-hedge', exchange: 'bybit', active: true, testnet: false, connected: true },
    { name: 'okx-cold', exchange: 'okx', active: false, testnet: false, connected: false },
  ],
  positions: [
    {
      account: 'binance-main',
      symbol: 'BTCUSDT',
      side: 'long',
      contracts: 1,
      nativeContracts: 1,
      entryPrice: 60_000,
      markPrice: 62_000,
      notional: 62_000,
      unrealizedPnl: 2_000,
      liquidationPrice: 30_000,
      leverage: 5,
      positionMode: 'one-way',
    },
    {
      account: 'bybit-hedge',
      symbol: 'ETHUSD',
      side: 'long',
      contracts: 100,
      // Inverse: native is notional/contractSize, not the base contract count.
      nativeContracts: 100,
      entryPrice: 3_000,
      markPrice: 3_100,
      notional: 100,
      unrealizedPnl: 1,
      liquidationPrice: 1_500,
      leverage: 10,
      positionMode: 'hedged',
    },
    {
      account: 'bybit-hedge',
      symbol: 'ETHUSD',
      side: 'short',
      contracts: 40,
      nativeContracts: 40,
      entryPrice: 3_200,
      markPrice: 3_100,
      notional: 40,
      unrealizedPnl: 1,
      liquidationPrice: 6_000,
      leverage: 10,
      positionMode: 'hedged',
    },
  ],
  orders: [
    {
      account: 'binance-main',
      id: 'o1',
      symbol: 'BTCUSDT',
      side: 'buy',
      type: 'limit',
      amount: 0.5,
      filled: 0,
      remaining: 0.5,
      nativeRemaining: 0.5,
      price: 58_000,
      reduceOnly: false,
    },
    {
      account: 'binance-main',
      id: 'o2',
      symbol: 'ETHUSDT',
      side: 'sell',
      type: 'limit',
      amount: 2,
      filled: 0,
      remaining: 2,
      nativeRemaining: 2,
      price: 4_000,
      reduceOnly: true,
    },
  ],
  balances: [{ account: 'binance-main', currency: 'USDT', total: 10_000, free: 8_000, used: 2_000, upnl: 2_000 }],
  fatFingerNotional: 100_000,
  markets: [
    {
      symbol: 'BTCUSDT',
      account: 'binance-main',
      lastPrice: 62_000,
      amountStep: 0.001,
      priceStep: 0.1,
      minAmount: 0.001,
      maxAmount: 1_000,
      maxLeverage: 125,
      contractSize: 1,
      isInverse: false,
      isSpot: false,
    },
    {
      symbol: 'ETHUSDT',
      account: 'binance-main',
      lastPrice: 3_100,
      amountStep: 0.01,
      priceStep: 0.01,
      minAmount: 0.01,
      maxAmount: 5_000,
      maxLeverage: 100,
      contractSize: 1,
      isInverse: false,
      isSpot: true,
    },
    {
      symbol: 'ETHUSD',
      account: 'bybit-hedge',
      lastPrice: 3_100,
      amountStep: 1,
      priceStep: 0.05,
      minAmount: 1,
      maxAmount: 100_000,
      maxLeverage: 50,
      contractSize: 1,
      isInverse: true,
      isSpot: false,
    },
  ],
  ...over,
});

const plan = (...steps: OemsIntent[]): OemsPlan => ({ reply: '', steps, reads: [] });
const resolve = (p: OemsPlan, over: Partial<OemsContext> = {}) => resolveOemsPlan(p, context(over), { now: NOW });
const resolveWith = (p: OemsPlan, overrides: readonly OemsOverride[], over: Partial<OemsContext> = {}) =>
  resolveOemsPlan(p, context(over), { now: NOW, overrides });

describe('reduce all my positions by 30%', () => {
  const result = resolve(plan({ kind: 'reduce_positions', scope: {}, percent: 30 }));

  it('produces one reduce-only close per position, both hedged legs included', () => {
    expect(result.rejections).toEqual([]);
    expect(result.commands.map((command) => command.args)).toEqual([
      [{ symbol: 'BTCUSDT', side: 'sell', type: 'market', amount: 0.3, reduceOnly: true }, 'binance-main'],
      [{ symbol: 'ETHUSD', side: 'sell', type: 'market', amount: 30, reduceOnly: true }, 'bybit-hedge'],
      [{ symbol: 'ETHUSD', side: 'buy', type: 'market', amount: 12, reduceOnly: true }, 'bybit-hedge'],
    ]);
  });

  it('closes with a reduce-only market order sized in the venue own units', () => {
    for (const command of result.commands) {
      expect(command.method).toBe('placeOrder');
      expect(Object.keys(command.args[0] as object).sort()).toEqual([
        'amount',
        'reduceOnly',
        'side',
        'symbol',
        'type',
      ]);
    }
  });

  it('says what will happen in plain terms', () => {
    expect(result.commands[0].preview).toBe('Reduce BTCUSDT long on binance-main by 0.3 of 1 (30%)');
  });
});

describe('reduce_positions guards', () => {
  it('refuses 100% and points at the verb that means it', () => {
    const result = resolve(plan({ kind: 'reduce_positions', scope: {}, percent: 100 }));
    expect(result.commands).toEqual([]);
    expect(result.rejections[0].reason).toMatch(/close_positions/);
  });

  it('refuses a size that rounds below the market minimum', () => {
    // 1% of 1 BTC is 0.01 -> rounds fine, so push the minimum up instead.
    const markets = context().markets.map((market) =>
      market.symbol === 'BTCUSDT' ? { ...market, minAmount: 0.05 } : market,
    );
    const result = resolve(plan({ kind: 'reduce_positions', scope: { symbols: ['BTCUSDT'] }, percent: 1 }), {
      markets,
    });
    expect(result.commands).toEqual([]);
    expect(result.rejections[0].reason).toMatch(/below the 0.05 minimum/);
  });

  it('sizes against nativeContracts, not the base contract count', () => {
    // An OKX-style `contractSize 0.01` market: 5 BTC of position is 500
    // contracts to the venue. Sizing off `contracts` would close a hundredth.
    const positions = context().positions.map((position) =>
      position.symbol === 'BTCUSDT' ? { ...position, contracts: 5, nativeContracts: 500 } : position,
    );
    const markets = context().markets.map((market) =>
      market.symbol === 'BTCUSDT' ? { ...market, contractSize: 0.01, amountStep: 1, minAmount: 1 } : market,
    );
    const result = resolve(plan({ kind: 'reduce_positions', scope: { symbols: ['BTCUSDT'] }, percent: 30 }), {
      positions,
      markets,
    });
    expect((result.commands[0].args[0] as { amount: number }).amount).toBe(150);
  });

  it('skips the venue-minimum check where the limit unit is adapter-dependent', () => {
    // `ctVal 100`: OKX reports `min` in base and `max` in native, KuCoin
    // reports both natively. No conversion is right for all of them, so the
    // check is skipped rather than guessed and the venue does the refusing.
    const positions = context().positions.map((position) =>
      position.symbol === 'BTCUSDT' ? { ...position, contracts: 500, nativeContracts: 5 } : position,
    );
    const markets = context().markets.map((market) =>
      market.symbol === 'BTCUSDT'
        ? { ...market, contractSize: 100, amountStep: 1, minAmount: 150, maxAmount: 0 }
        : market,
    );
    const result = resolve(plan({ kind: 'reduce_positions', scope: { symbols: ['BTCUSDT'] }, percent: 40 }), {
      positions,
      markets,
    });
    expect(result.rejections).toEqual([]);
    expect((result.commands[0].args[0] as { amount: number }).amount).toBe(2);
  });

  it('refuses a flat position row instead of sending a zero-size close', () => {
    const positions = context().positions.map((position) =>
      position.symbol === 'BTCUSDT' ? { ...position, contracts: 0, nativeContracts: 0 } : position,
    );
    const result = resolve(plan({ kind: 'reduce_positions', scope: { symbols: ['BTCUSDT'] }, percent: 30 }), {
      positions,
    });
    expect(result.commands).toEqual([]);
    expect(result.rejections[0].reason).toMatch(/no size to close/);
  });

  it('says so when nothing matches', () => {
    const result = resolve(plan({ kind: 'reduce_positions', scope: { symbols: ['SOLUSDT'] }, percent: 30 }));
    expect(result.rejections[0].reason).toBe('no open positions match that description');
  });

  it('matches symbols case-insensitively, as the host market lookup does', () => {
    const result = resolve(plan({ kind: 'reduce_positions', scope: { symbols: ['btcusdt'] }, percent: 30 }));
    expect(result.commands).toHaveLength(1);
  });
});

describe('close_positions', () => {
  it('sends no contracts, so the host closes what it holds now', () => {
    const result = resolve(plan({ kind: 'close_positions', scope: { accounts: ['binance-main'] } }));
    expect(result.commands[0].args).toEqual([{ symbol: 'BTCUSDT', side: 'long' }, 'binance-main']);
  });

  it('sizes the close itself on a linear multi-unit contract market too', () => {
    const positions = context().positions.map((position) =>
      position.symbol === 'BTCUSDT' ? { ...position, contracts: 500, nativeContracts: 5 } : position,
    );
    const markets = context().markets.map((market) =>
      market.symbol === 'BTCUSDT' ? { ...market, contractSize: 100, amountStep: 1, minAmount: 0 } : market,
    );
    const result = resolve(plan({ kind: 'close_positions', scope: { symbols: ['BTCUSDT'] } }), {
      positions,
      markets,
    });
    expect(result.commands[0].method).toBe('placeOrder');
    expect((result.commands[0].args[0] as { amount: number }).amount).toBe(5);
  });

  it('refuses a close on a symbol with no market row rather than guessing the unit', () => {
    const markets = context().markets.filter((market) => market.symbol !== 'BTCUSDT');
    const result = resolve(plan({ kind: 'close_positions', scope: { symbols: ['BTCUSDT'] } }), { markets });
    expect(result.commands).toEqual([]);
    expect(result.rejections[0].reason).toMatch(/no market data/);
  });

  it('sizes the close itself on an inverse market, where closePosition would not', () => {
    const result = resolve(plan({ kind: 'close_positions', scope: { symbols: ['ETHUSD'] } }));
    expect(result.rejections).toEqual([]);
    expect(result.commands.map((command) => command.method)).toEqual(['placeOrder', 'placeOrder']);
    expect((result.commands[0].args[0] as { amount: number }).amount).toBe(100);
  });
});

describe('cancel_orders', () => {
  it('sends one id per order, grouped by account', () => {
    const result = resolve(plan({ kind: 'cancel_orders', scope: {} }));
    expect(result.commands).toHaveLength(1);
    expect(result.commands[0].method).toBe('cancelOrders');
    expect(result.commands[0].args).toEqual([{ ids: ['o1', 'o2'] }, 'binance-main']);
  });

  it('uses the single-order verb for a single order', () => {
    const result = resolve(plan({ kind: 'cancel_orders', scope: { symbols: ['BTCUSDT'] } }));
    expect(result.commands[0].method).toBe('cancelOrder');
    expect(result.commands[0].args).toEqual(['o1', { account: 'binance-main', symbol: 'BTCUSDT' }]);
  });

  it('chunks at the host batch cap', () => {
    const orders = Array.from({ length: 30 }, (_, index) => ({
      account: 'binance-main',
      id: `x${index}`,
      symbol: 'BTCUSDT',
      side: 'buy' as const,
      type: 'limit' as const,
      amount: 1,
      filled: 0,
      remaining: 1,
      nativeRemaining: 1,
      price: 1,
      reduceOnly: false,
    }));
    const result = resolve(plan({ kind: 'cancel_orders', scope: {} }), { orders });
    expect(result.commands).toHaveLength(2);
    expect((result.commands[0].args[0] as { ids: unknown[] }).ids.length).toBe(25);
    expect((result.commands[1].args[0] as { ids: unknown[] }).ids.length).toBe(5);
  });
});

describe('place_order', () => {
  it('puts symbol first and keeps symbol/side out of the options bag', () => {
    const result = resolve(
      plan({
        kind: 'place_order',
        account: 'binance-main',
        symbol: 'BTCUSDT',
        side: 'buy',
        orderType: 'limit',
        amount: 0.1234,
        price: 61_000.07,
      }),
    );
    expect(result.commands[0].args).toEqual([
      { symbol: 'BTCUSDT', side: 'buy', type: 'limit', amount: 0.123, price: 61_000.1 },
      'binance-main',
    ]);
  });

  it('refuses a limit with no price and a market with one', () => {
    const noPrice = resolve(
      plan({
        kind: 'place_order',
        account: 'binance-main',
        symbol: 'BTCUSDT',
        side: 'buy',
        orderType: 'limit',
        amount: 1,
      }),
    );
    expect(noPrice.rejections[0].reason).toMatch(/needs a positive price/);

    const pricedMarket = resolve(
      plan({
        kind: 'place_order',
        account: 'binance-main',
        symbol: 'BTCUSDT',
        side: 'buy',
        orderType: 'market',
        amount: 1,
        price: 100,
      }),
    );
    expect(pricedMarket.rejections[0].reason).toMatch(/cannot carry a price/);
  });

  it('refuses reduce-only on a spot market', () => {
    const result = resolve(
      plan({
        kind: 'place_order',
        account: 'binance-main',
        symbol: 'ETHUSDT',
        side: 'sell',
        orderType: 'market',
        amount: 1,
        reduceOnly: true,
      }),
    );
    expect(result.rejections[0].reason).toMatch(/spot market/);
  });

  it('refuses an account that is switched off or not connected', () => {
    const result = resolve(
      plan({
        kind: 'place_order',
        account: 'okx-cold',
        symbol: 'BTCUSDT',
        side: 'buy',
        orderType: 'market',
        amount: 1,
      }),
    );
    expect(result.rejections[0].reason).toMatch(/switched off/);
  });

  it('refuses an opening order over the fat-finger limit, which the host would throw on', () => {
    const result = resolve(
      plan({
        kind: 'place_order',
        account: 'binance-main',
        symbol: 'BTCUSDT',
        side: 'buy',
        orderType: 'market',
        amount: 10,
      }),
    );
    expect(result.rejections[0].reason).toMatch(/fat-finger limit/);
  });

  it('refuses a market order it cannot price against the fat-finger limit', () => {
    const base = context();
    const result = resolve(
      plan({ kind: 'place_order', account: 'binance-main', symbol: 'BTCUSDT', side: 'buy', orderType: 'market', amount: 0.001 }),
      { markets: base.markets.map((m) => (m.symbol === 'BTCUSDT' ? { ...m, lastPrice: 0 } : m)) },
    );
    expect(result.commands).toHaveLength(0);
    expect(result.rejections[0].reason).toMatch(/no price for BTCUSDT/);
  });

  it('exempts a reduce-only order from fat-finger, as the host does', () => {
    const result = resolve(
      plan({
        kind: 'place_order',
        account: 'binance-main',
        symbol: 'BTCUSDT',
        side: 'sell',
        orderType: 'market',
        amount: 10,
        reduceOnly: true,
      }),
    );
    expect(result.rejections).toEqual([]);
  });

  it('refuses reduce-only where there is no position to reduce', () => {
    const result = resolve(
      plan({
        kind: 'place_order',
        account: 'binance-main',
        symbol: 'BTCUSDT',
        side: 'sell',
        orderType: 'market',
        amount: 0.1,
        reduceOnly: true,
      }),
      { positions: [] },
    );
    expect(result.rejections[0].reason).toMatch(/no open BTCUSDT position/);
  });

  it('refuses a symbol with no market row rather than guessing precision', () => {
    const result = resolve(
      plan({
        kind: 'place_order',
        account: 'binance-main',
        symbol: 'SOLUSDT',
        side: 'buy',
        orderType: 'market',
        amount: 1,
      }),
    );
    expect(result.rejections[0].reason).toMatch(/no market data/);
  });
});

describe('place_stop_loss', () => {
  it('places a stop below the market to protect a long', () => {
    const result = resolve(
      plan({
        kind: 'place_stop_loss',
        account: 'binance-main',
        symbol: 'BTCUSDT',
        side: 'sell',
        amount: 1,
        price: 58_000,
      }),
    );
    expect(result.commands[0].args).toEqual([
      { symbol: 'BTCUSDT', side: 'sell', type: 'stop_market', amount: 1, price: 58_000, reduceOnly: true },
      'binance-main',
    ]);
  });

  it('refuses a trigger on the wrong side, which would become a take-profit', () => {
    const result = resolve(
      plan({
        kind: 'place_stop_loss',
        account: 'binance-main',
        symbol: 'BTCUSDT',
        side: 'sell',
        amount: 1,
        price: 65_000,
      }),
    );
    expect(result.rejections[0].reason).toMatch(/fill immediately/);
  });

  it('refuses a stop with no position to protect', () => {
    const result = resolve(
      plan({
        kind: 'place_stop_loss',
        account: 'binance-main',
        symbol: 'ETHUSDT',
        side: 'sell',
        amount: 1,
        price: 3_000,
      }),
    );
    expect(result.rejections[0].reason).toMatch(/to protect/);
  });
});

describe('amend_order', () => {
  it('sends exactly one of price or amount, keyed by id', () => {
    const result = resolve(plan({ kind: 'amend_order', account: 'binance-main', orderId: 'o1', price: 57_999.96 }));
    expect(result.commands[0].args).toEqual(['o1', { update: { price: 58_000 } }, 'binance-main']);
    expect(Object.keys((result.commands[0].args[1] as { update: object }).update)).toEqual(['price']);
  });

  it('refuses an id the context does not hold', () => {
    const result = resolve(plan({ kind: 'amend_order', account: 'binance-main', orderId: 'nope', amount: 1 }));
    expect(result.rejections[0].reason).toMatch(/no open order nope/);
  });
});

describe('set_leverage', () => {
  it('omits the side rather than sending undefined, which would serialise to null', () => {
    const result = resolve(plan({ kind: 'set_leverage', account: 'binance-main', symbol: 'BTCUSDT', leverage: 10 }));
    expect(result.commands[0].args).toEqual(['BTCUSDT', 10, 'binance-main']);
  });

  it('passes the side through when one is named', () => {
    const result = resolve(
      plan({ kind: 'set_leverage', account: 'bybit-hedge', symbol: 'ETHUSD', leverage: 10, side: 'long' }),
    );
    expect(result.commands[0].args).toEqual(['ETHUSD', 10, 'bybit-hedge', 'long']);
  });

  it('refuses spot, over-limit leverage, and an unsided hedged account', () => {
    expect(
      resolve(plan({ kind: 'set_leverage', account: 'binance-main', symbol: 'ETHUSDT', leverage: 5 })).rejections[0]
        .reason,
    ).toMatch(/spot market/);

    expect(
      resolve(plan({ kind: 'set_leverage', account: 'binance-main', symbol: 'BTCUSDT', leverage: 500 })).rejections[0]
        .reason,
    ).toMatch(/at most 125x/);

    expect(
      resolve(plan({ kind: 'set_leverage', account: 'bybit-hedge', symbol: 'ETHUSD', leverage: 5 })).rejections[0]
        .reason,
    ).toMatch(/hedge mode/);
  });
});

describe('staleness', () => {
  it('refuses the whole plan rather than resolving against an old book', () => {
    const result = resolveOemsPlan(plan({ kind: 'reduce_positions', scope: {}, percent: 30 }), context(), {
      now: NOW + 60_000,
    });
    expect(result.commands).toEqual([]);
    expect(result.rejections[0].reason).toMatch(/60s old/);
  });
});

describe('partial resolution', () => {
  it('keeps the steps it can and reports the ones it cannot', () => {
    const result = resolve(
      plan(
        { kind: 'reduce_positions', scope: { accounts: ['binance-main'] }, percent: 30 },
        { kind: 'set_leverage', account: 'binance-main', symbol: 'ETHUSDT', leverage: 5 },
      ),
    );
    expect(result.commands).toHaveLength(1);
    expect(result.rejections).toEqual([
      { step: 1, kind: 'set_leverage', reason: 'ETHUSDT on binance-main is a spot market and has no leverage' },
    ]);
  });
});

/**
 * An override is the number a user typed over the one the model derived. The
 * property under test throughout is that it changes the NUMBER and nothing
 * else: every check that makes a size safe still runs against it.
 */
describe('user overrides', () => {
  const reduce30 = plan({ kind: 'reduce_positions', scope: {}, percent: 30 });

  it('replaces a derived close size with the one the user typed', () => {
    const result = resolveWith(reduce30, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'sell', amount: 0.4 },
    ]);
    expect(result.commands[0].args[0]).toMatchObject({ symbol: 'BTCUSDT', amount: 0.4 });
  });

  it('leaves every other row derived — an override is per order, not per card', () => {
    const result = resolveWith(reduce30, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'sell', amount: 0.4 },
    ]);
    expect(result.commands[1].args[0]).toMatchObject({ symbol: 'ETHUSD', amount: 30 });
    expect(result.commands[2].args[0]).toMatchObject({ symbol: 'ETHUSD', amount: 12 });
  });

  it('matches on side, so one leg of a hedge cannot take the other leg edit', () => {
    const result = resolveWith(reduce30, [{ step: 0, symbol: 'ETHUSD', account: 'bybit-hedge', side: 'buy', amount: 5 }]);
    // The long leg closes with a SELL and is untouched; only the short leg's
    // closing BUY takes the edit.
    expect(result.commands[1].args[0]).toMatchObject({ side: 'sell', amount: 30 });
    expect(result.commands[2].args[0]).toMatchObject({ side: 'buy', amount: 5 });
  });

  it('still clamps an edited close to the position it reduces', () => {
    const result = resolveWith(reduce30, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'sell', amount: 99 },
    ]);
    // The position is 1 contract. Asking for 99 cannot send 99.
    expect(result.commands[0].args[0]).toMatchObject({ amount: 1 });
  });

  it('still rounds an edited size to the lot step', () => {
    const result = resolveWith(reduce30, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'sell', amount: 0.40049 },
    ]);
    expect(result.commands[0].args[0]).toMatchObject({ amount: 0.4 });
  });

  it('still refuses an edited size below the market minimum', () => {
    const result = resolveWith(reduce30, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'sell', amount: 0.0000001 },
    ]);
    expect(result.commands.some((command) => (command.args[0] as { symbol: string }).symbol === 'BTCUSDT')).toBe(
      false,
    );
    expect(result.rejections.map((rejection) => rejection.reason).join(' ')).toMatch(/minimum|not a size/);
  });

  it('states the number it will send, and drops the percentage it no longer is', () => {
    const result = resolveWith(reduce30, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'sell', amount: 0.4 },
    ]);
    expect(result.commands[0].preview).toContain('0.4');
    expect(result.commands[0].preview).not.toContain('30%');
    // The untouched rows keep theirs.
    expect(result.commands[1].preview).toContain('30%');
  });

  it('still enforces the fat-finger limit on an edited entry', () => {
    const big = plan({
      kind: 'place_order',
      account: 'binance-main',
      symbol: 'BTCUSDT',
      side: 'buy',
      orderType: 'market',
      amount: 0.1,
    });
    // 0.1 @ 62,000 is fine; 10 is 620,000 against a 100,000 limit.
    expect(resolve(big).commands).toHaveLength(1);
    const edited = resolveWith(big, [{ step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'buy', amount: 10 }]);
    expect(edited.commands).toEqual([]);
    expect(edited.rejections[0].reason).toMatch(/fat-finger/);
  });

  it('takes an edited limit price, and rounds it to the tick', () => {
    const limit = plan({
      kind: 'place_order',
      account: 'binance-main',
      symbol: 'BTCUSDT',
      side: 'buy',
      orderType: 'limit',
      amount: 0.1,
      price: 60_000,
    });
    const edited = resolveWith(limit, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'buy', price: 59_000.07 },
    ]);
    expect(edited.commands[0].args[0]).toMatchObject({ price: 59_000.1 });
  });

  it('does not put a price on a market order', () => {
    const market = plan({
      kind: 'place_order',
      account: 'binance-main',
      symbol: 'BTCUSDT',
      side: 'buy',
      orderType: 'market',
      amount: 0.1,
    });
    const edited = resolveWith(market, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'buy', price: 59_000 },
    ]);
    expect(edited.rejections).toEqual([]);
    expect(edited.commands[0].args[0]).not.toHaveProperty('price');
  });

  it('routes an edited linear close off the closePosition fast path', () => {
    const close = plan({ kind: 'close_positions', scope: { symbols: ['BTCUSDT'] } });
    // Unedited, a plain linear market takes `closePosition` and carries no size.
    expect(resolve(close).commands[0].method).toBe('closePosition');
    // Edited, it must become a sized order or the number has nowhere to go.
    const edited = resolveWith(close, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'sell', amount: 0.25 },
    ]);
    expect(edited.commands[0].method).toBe('placeOrder');
    expect(edited.commands[0].args[0]).toMatchObject({ amount: 0.25, reduceOnly: true });
  });

  it('edits ONE step, not every step that shares a symbol and side', () => {
    // The collision this key exists to stop: a reduce and a stop on the same
    // symbol, account and side. Keyed on symbol+side alone, editing the reduce
    // silently shrank the stop too and left the rest of the position
    // unprotected.
    const both = plan(
      { kind: 'reduce_positions', scope: { symbols: ['BTCUSDT'] }, percent: 50 },
      {
        kind: 'place_stop_loss',
        account: 'binance-main',
        symbol: 'BTCUSDT',
        side: 'sell',
        amount: 0.5,
        price: 55_000,
      },
    );
    const edited = resolveWith(both, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'sell', amount: 0.2 },
    ]);

    expect(edited.commands[0].args[0]).toMatchObject({ amount: 0.2 });
    expect(edited.commands[1].args[0]).toMatchObject({ amount: 0.5 });
  });

  it('stamps every command with the step that produced it', () => {
    const both = plan(
      { kind: 'reduce_positions', scope: { symbols: ['BTCUSDT'] }, percent: 50 },
      { kind: 'set_leverage', account: 'binance-main', symbol: 'BTCUSDT', leverage: 5 },
    );
    expect(resolve(both).commands.map((command) => command.step)).toEqual([0, 1]);
  });

  it('refuses to edit a hedged close rather than guess which leg', () => {
    // The unedited fast path names the leg via `closePosition({symbol, side})`.
    // `PlaceOrderOptions` has no field that can, so an edit that dropped to a
    // bare reduce-only order could shrink the wrong side of a hedge.
    const hedgedLinear = {
      positions: [
        {
          account: 'binance-main',
          symbol: 'BTCUSDT',
          side: 'long' as const,
          contracts: 1,
          nativeContracts: 1,
          entryPrice: 60_000,
          markPrice: 62_000,
          notional: 62_000,
          unrealizedPnl: 0,
          liquidationPrice: 30_000,
          leverage: 5,
          positionMode: 'hedged' as const,
        },
      ],
    };
    const close = plan({ kind: 'close_positions', scope: { symbols: ['BTCUSDT'] } });

    expect(resolve(close, hedgedLinear).commands[0].method).toBe('closePosition');

    const edited = resolveWith(
      close,
      [{ step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'sell', amount: 0.25 }],
      hedgedLinear,
    );
    expect(edited.commands).toEqual([]);
    expect(edited.rejections[0].reason).toMatch(/hedged/);
  });

  it.each([Number.POSITIVE_INFINITY, Number.NaN, 0, -1])('ignores a %s size and keeps the derived one', (bad) => {
    // An override never passes through `parseOemsPlan`, so nothing else checks
    // it. `roundToStep(Infinity)` is `Infinity` and `Infinity > 0` passes, and
    // min/max are skipped whenever `contractSize !== 1` — so an unchecked
    // infinity reached the venue on an OKX-shaped market.
    const result = resolveWith(reduce30, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'sell', amount: bad },
    ]);
    expect(result.commands[0].args[0]).toMatchObject({ amount: 0.3 });
  });

  it('refuses a price typed onto a market close rather than dropping it', () => {
    const result = resolveWith(reduce30, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'sell', price: 61_000 },
    ]);
    expect(result.commands.some((command) => command.preview.includes('BTCUSDT'))).toBe(false);
    expect(result.rejections.map((rejection) => rejection.reason).join(' ')).toMatch(/takes no price/);
  });

  it('marks a row editable only where an edit would be honoured', () => {
    // `editable` IS the pencil. Advertising it on a row the resolver would
    // refuse produces an affordance whose only outcome is a rejection.
    const mixed = plan(
      { kind: 'reduce_positions', scope: { symbols: ['BTCUSDT'] }, percent: 30 },
      { kind: 'cancel_orders', scope: {} },
      { kind: 'set_leverage', account: 'binance-main', symbol: 'BTCUSDT', leverage: 5 },
    );
    const result = resolve(mixed);
    const byMethod = Object.fromEntries(result.commands.map((command) => [command.method, command]));

    expect(byMethod.placeOrder.editable).toMatchObject({ symbol: 'BTCUSDT', side: 'sell' });
    // A close takes no price, so it must not offer one.
    expect(byMethod.placeOrder.editable).not.toHaveProperty('price');
    expect(byMethod.setLeverage?.editable).toBeUndefined();
    expect(byMethod.cancelOrders?.editable ?? byMethod.cancelOrder?.editable).toBeUndefined();
  });

  it('does not offer an edit on a hedged close it would refuse', () => {
    const hedgedLinear = {
      positions: [
        {
          account: 'binance-main',
          symbol: 'BTCUSDT',
          side: 'long' as const,
          contracts: 1,
          nativeContracts: 1,
          entryPrice: 60_000,
          markPrice: 62_000,
          notional: 62_000,
          unrealizedPnl: 0,
          liquidationPrice: 30_000,
          leverage: 5,
          positionMode: 'hedged' as const,
        },
      ],
    };
    const result = resolve(plan({ kind: 'close_positions', scope: { symbols: ['BTCUSDT'] } }), hedgedLinear);
    expect(result.commands[0].method).toBe('closePosition');
    expect(result.commands[0].editable).toBeUndefined();
  });

  it('reports the EDITED number as the row editable value, so editing twice is idempotent', () => {
    const entry = plan({
      kind: 'place_order',
      account: 'binance-main',
      symbol: 'BTCUSDT',
      side: 'buy',
      orderType: 'limit',
      amount: 0.1,
      price: 60_000,
    });
    const edited = resolveWith(entry, [
      { step: 0, symbol: 'BTCUSDT', account: 'binance-main', side: 'buy', amount: 0.25 },
    ]);
    expect(edited.commands[0].editable).toMatchObject({ amount: 0.25, price: 60_000 });
  });

  it('ignores an override that matches no order', () => {
    const result = resolveWith(reduce30, [{ step: 0, symbol: 'DOGEUSDT', account: 'binance-main', side: 'sell', amount: 5 }]);
    expect(result.commands.map((command) => (command.args[0] as { amount: number }).amount)).toEqual([0.3, 30, 12]);
  });
});
