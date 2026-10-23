import { describe, expect, it } from 'vitest';

import { SPOT_ASSET_INDEX_OFFSET } from '../../exchanges/hyperliquid/hyperliquid.constants';
import { HyperliquidExchange } from '../../exchanges/hyperliquid/hyperliquid.exchange';
import { DefaultStore } from '../../store/store.base';
import { ExchangeOptions, OrderSide, OrderType } from '../../types';

/**
 * Oracle: live `POST /info { type: 'spotMetaAndAssetCtxs' }` against
 * api.hyperliquid.xyz on 2026-07-27.
 *
 * The load-bearing property this locks in: the response's asset-context array
 * is NOT positionally aligned with `meta.universe`. The live response carried
 * 316 universe entries against 707 contexts, and a positional (`assets[i]`)
 * join mismatched 245 of the 316 pairs. Contexts must be joined on their
 * `coin` field. The perp `metaAndAssetCtxs` path IS positional, so the
 * temptation to make the two paths symmetric is real and wrong.
 *
 * The fixture below preserves that shape in miniature: contexts are supplied
 * out of universe order and with extra entries, so a positional join produces
 * demonstrably wrong prices.
 */
const spotMetaFixture = {
  tokens: [
    { name: 'USDC', index: 0, szDecimals: 8, weiDecimals: 8, tokenId: '0x0', isCanonical: true },
    { name: 'PURR', index: 1, szDecimals: 0, weiDecimals: 5, tokenId: '0x1', isCanonical: true },
    { name: 'HFUN', index: 2, szDecimals: 2, weiDecimals: 8, tokenId: '0x2', isCanonical: false },
    { name: 'LICK', index: 3, szDecimals: 0, weiDecimals: 8, tokenId: '0x3', isCanonical: false },
    { name: 'USDH', index: 4, szDecimals: 8, weiDecimals: 8, tokenId: '0x4', isCanonical: false },
    // Unit-bridged: HyperCore has no native ETH, so the on-chain token is
    // `UETH` while Hyperliquid's own UI displays it as `ETH`.
    {
      name: 'UETH',
      index: 5,
      szDecimals: 4,
      weiDecimals: 9,
      tokenId: '0x5',
      isCanonical: false,
      fullName: 'Unit Ethereum',
    },
    // Unit-bridged, but a DISTINCT native token already owns the bare name.
    {
      name: 'UPUMP',
      index: 6,
      szDecimals: 2,
      weiDecimals: 8,
      tokenId: '0x6',
      isCanonical: false,
      fullName: 'Unit Pump Fun',
    },
    { name: 'PUMP', index: 7, szDecimals: 2, weiDecimals: 8, tokenId: '0x7', isCanonical: false, fullName: 'Pump' },
    // Starts with U and is not Unit-bridged — must never be stripped.
    { name: 'UNI', index: 8, szDecimals: 2, weiDecimals: 8, tokenId: '0x8', isCanonical: false, fullName: 'Uniswap' },
  ],
  universe: [
    { name: 'PURR/USDC', tokens: [1, 0], index: 0, isCanonical: true },
    { name: '@1', tokens: [2, 0], index: 1, isCanonical: false },
    { name: '@2', tokens: [3, 0], index: 2, isCanonical: false },
    // Same BASE as `@1`, different quote. Hardcoding the quote to USDC would
    // render this as a second `HFUN/USDC` and clobber the real one in the
    // symbol-keyed `marketsBySymbol`. Live data has five such collisions.
    { name: '@3', tokens: [2, 4], index: 3, isCanonical: false },
    { name: '@4', tokens: [5, 0], index: 4, isCanonical: false }, // UETH  -> displays as ETH
    { name: '@5', tokens: [6, 0], index: 5, isCanonical: false }, // UPUMP -> stays UPUMP
    { name: '@6', tokens: [7, 0], index: 6, isCanonical: false }, // PUMP  (the distinct native token)
    { name: '@7', tokens: [8, 0], index: 7, isCanonical: false }, // UNI   -> stays UNI
  ],
};

// Deliberately shuffled relative to `universe`, plus a trailing context with
// no universe entry — mirroring the real 316-vs-707 mismatch.
const spotAssetCtxsFixture = [
  { coin: '@2', midPx: '0.00005088', markPx: '0.00005081', prevDayPx: '0.00005000', dayNtlVlm: '0.0' },
  { coin: 'PURR/USDC', midPx: '0.0686925', markPx: '0.06872', prevDayPx: '0.063525', dayNtlVlm: '369448.5' },
  { coin: '@99', midPx: '1.0', markPx: '1.0', prevDayPx: '1.0', dayNtlVlm: '0.0' },
  { coin: '@1', midPx: '18.272', markPx: '18.254', prevDayPx: '18.089', dayNtlVlm: '16660.6' },
  { coin: '@3', midPx: '18.301', markPx: '18.300', prevDayPx: '18.100', dayNtlVlm: '120.0' },
  { coin: '@4', midPx: '1963.15', markPx: '1962.0', prevDayPx: '1900.0', dayNtlVlm: '13676432.0' },
  { coin: '@5', midPx: '0.0020322', markPx: '0.00203', prevDayPx: '0.0019', dayNtlVlm: '1411950.0' },
  { coin: '@6', midPx: '0.5', markPx: '0.5', prevDayPx: '0.49', dayNtlVlm: '1000.0' },
  { coin: '@7', midPx: '6.12', markPx: '6.1', prevDayPx: '6.0', dayNtlVlm: '5000.0' },
];

const createExchange = (enableSpotMarkets = true): HyperliquidExchange => {
  const options: ExchangeOptions = {
    key: '0xkey',
    // Valid 32-byte hex so EIP-712 signing runs — throwaway test key.
    secret: '0x0123456789012345678901234567890123456789012345678901234567890123',
    applicationId: '0xuser',
    testnet: true,
    enableSpotMarkets,
    tuning: { consoleLogging: false },
  };

  const exchange = new HyperliquidExchange('hyperliquid', options, new DefaultStore());

  (exchange as any).infoXhr = {
    post: async () => ({ data: [spotMetaFixture, spotAssetCtxsFixture] }),
  };

  return exchange;
};

describe('hyperliquid spot markets merged into the perp container', () => {
  it('loads no spot markets unless the account opted in', async () => {
    const exchange = createExchange(false);
    let requested = false;
    (exchange as any).infoXhr = {
      post: async () => {
        requested = true;
        return { data: [spotMetaFixture, spotAssetCtxsFixture] };
      },
    };

    const result = await (exchange as any).fetchSpotMarketsAndTickers();
    exchange.dispose();

    // Default-off must be free: no markets AND no request, so a perp trader's
    // market list and request volume are unchanged by the merge.
    expect(result).toEqual({ markets: [], tickers: [] });
    expect(requested).toBe(false);
  });

  it('joins asset contexts by coin, not by position', async () => {
    const exchange = createExchange();
    const { markets, tickers } = await (exchange as any).fetchSpotMarketsAndTickers();
    exchange.dispose();

    expect(markets).toHaveLength(8);

    const tickerBySymbol = Object.fromEntries(tickers.map((t: any) => [t.symbol, t]));

    // Each pair must carry ITS OWN price. A positional join would have given
    // PURR the '@2' context (0.00005088) and '@2' the PURR context.
    expect(tickerBySymbol['PURR/USDC'].last).toBeCloseTo(0.0686925, 9);
    expect(tickerBySymbol['HFUN/USDC'].last).toBeCloseTo(18.272, 6);
    expect(tickerBySymbol['LICK/USDC'].last).toBeCloseTo(0.00005088, 12);
  });

  it('uses spot dayNtlVlm as quote volume without multiplying by price again', async () => {
    const exchange = createExchange();
    const { tickers } = await (exchange as any).fetchSpotMarketsAndTickers();
    exchange.dispose();

    const tickerBySymbol = Object.fromEntries(tickers.map((t: any) => [t.symbol, t]));

    expect(tickerBySymbol['PURR/USDC'].quoteVolume).toBeCloseTo(369448.5, 6);
    expect(tickerBySymbol['PURR/USDC'].volume).toBeCloseTo(369448.5 / 0.0686925, 6);
  });

  it('displays Unit-bridged tokens under the name Hyperliquid shows', async () => {
    const exchange = createExchange();
    const { markets } = await (exchange as any).fetchSpotMarketsAndTickers();
    exchange.dispose();

    const bySymbol = Object.fromEntries(markets.map((m: any) => [m.symbol, m]));

    // HyperCore has no native ETH — `UETH` is Unit's bridged token, and
    // Hyperliquid's own UI renders it `ETH/USDC`. Mirror that for display
    // while the routing key stays the venue's real name.
    expect(bySymbol['UETH/USDC'].prettySymbol).toBe('ETH/USDC');
    // `base` carries the alias too: the market dropdown renders
    // `base || prettySymbol || symbol`, so leaving base raw would show
    // "UETH" and never reach prettySymbol.
    expect(bySymbol['UETH/USDC'].base).toBe('ETH');
    // `symbol` stays the venue-true routing key — it backs saved favourites,
    // layouts, TradingView symbols and the candle cache, and the alias is
    // data-dependent, so aliasing it would break saved refs if Hyperliquid
    // ever lists a real `ETH` token.
    expect(bySymbol['UETH/USDC'].symbol).toBe('UETH/USDC');
    expect(bySymbol['UETH/USDC'].info.hlTokenName).toBe('UETH');
  });

  it('never aliases a Unit token onto a name a distinct token already owns', async () => {
    const exchange = createExchange();
    const { markets } = await (exchange as any).fetchSpotMarketsAndTickers();
    exchange.dispose();

    const bySymbol = Object.fromEntries(markets.map((m: any) => [m.symbol, m]));

    // `UPUMP` is Unit Pump Fun, but an unrelated natively-issued `PUMP` token
    // exists. Stripping the prefix would present two different assets under
    // one name, so the bridged one keeps its prefix.
    expect(bySymbol['UPUMP/USDC'].prettySymbol).toBeUndefined();
    expect(bySymbol['PUMP/USDC']).toBeDefined();

    // `UNI` starts with U but is not Unit-bridged — a blind prefix strip would
    // have rendered it as "NI".
    expect(bySymbol['UNI/USDC'].prettySymbol).toBeUndefined();

    // Display names must stay as unique as the symbols they stand in for.
    const displayed = markets.map((m: any) => m.prettySymbol || m.symbol);
    expect(new Set(displayed).size).toBe(displayed.length);
  });

  it('uses the real quote token so same-base pairs cannot collide', async () => {
    const exchange = createExchange();
    const { markets } = await (exchange as any).fetchSpotMarketsAndTickers();
    exchange.dispose();

    const symbols = markets.map((m: any) => m.symbol);

    // HFUN trades against both USDC and USDH. Hardcoding the quote would emit
    // `HFUN/USDC` twice; `marketsBySymbol` is symbol-keyed, so one would
    // silently disappear.
    expect(symbols).toContain('HFUN/USDC');
    expect(symbols).toContain('HFUN/USDH');
    expect(new Set(symbols).size).toBe(symbols.length);

    const usdh = markets.find((m: any) => m.symbol === 'HFUN/USDH');
    expect(usdh.quote).toBe('USDH');
    expect(usdh.base).toBe('HFUN');
  });

  it('derives the order-wire asset index as spot index + 10000', async () => {
    const exchange = createExchange();
    const { markets } = await (exchange as any).fetchSpotMarketsAndTickers();
    exchange.dispose();

    const bySymbol = Object.fromEntries(markets.map((m: any) => [m.symbol, m]));

    expect(bySymbol['PURR/USDC'].info.hyperliquidId).toBe(0 + SPOT_ASSET_INDEX_OFFSET);
    expect(bySymbol['HFUN/USDC'].info.hyperliquidId).toBe(1 + SPOT_ASSET_INDEX_OFFSET);
    expect(bySymbol['LICK/USDC'].info.hyperliquidId).toBe(2 + SPOT_ASSET_INDEX_OFFSET);
  });

  it('produces symbols that cannot collide with the perp namespace', async () => {
    const exchange = createExchange();
    const { markets } = await (exchange as any).fetchSpotMarketsAndTickers();
    exchange.dispose();

    for (const market of markets) {
      expect(market.isSpot).toBe(true);
      expect(market.symbol).toContain('/');
      expect(market.symbol).not.toContain('-PERP');
      // Spot is never leveraged; the margin widget reads this.
      expect(market.limits.leverage.max).toBe(1);
    }
  });

  it('skips pairs with no midPx instead of publishing an unpriceable market', async () => {
    const exchange = createExchange();
    (exchange as any).infoXhr = {
      post: async () => ({
        data: [
          spotMetaFixture,
          [
            { coin: 'PURR/USDC', midPx: null, markPx: '0.06872', prevDayPx: '0.06', dayNtlVlm: '0' },
            ...spotAssetCtxsFixture.filter((c) => c.coin !== 'PURR/USDC'),
          ],
        ],
      }),
    };

    const { markets } = await (exchange as any).fetchSpotMarketsAndTickers();
    exchange.dispose();

    expect(markets.map((m: any) => m.symbol)).not.toContain('PURR/USDC');
    expect(markets).toHaveLength(7);
  });

  it('never synthesizes fake positions for spot markets', () => {
    const exchange = createExchange();

    const positions = (exchange as any).generateFakePositionsFromMarkets([
      { id: 'BTC', symbol: 'BTC-PERP', isSpot: false, info: { hyperliquidId: 0 } },
      { id: '@1', symbol: 'HFUN/USDC', isSpot: true, info: { hyperliquidId: 10001 } },
    ]);
    exchange.dispose();

    // Spot exposure is a coin balance, not a Position. Synthesizing spot rows
    // here would also feed DisplayedMarginPostprocessor, whose exposure sums
    // are defined over derivatives notional only.
    expect(positions.every((p: any) => p.symbol !== 'HFUN/USDC')).toBe(true);
    expect(positions.some((p: any) => p.symbol === 'BTC-PERP')).toBe(true);
  });

  it('never synthesizes a position from a SPOT fill', () => {
    // Spot fills arrive on the same non-dex-scoped `userFills` channel as perp
    // fills, with a finite `startPosition`. Once spot markets resolve in
    // `marketsById`, the optimistic-position path accepts them unless spot is
    // explicitly excluded — inventing a phantom long that corrupts
    // `balance.upnl` / free margin (order sizing) and DisplayedMargin
    // exposure, and persists until the next authoritative snapshot.
    const exchange = createExchange();
    exchange.store.update({
      markets: [
        {
          id: '@1',
          symbol: 'HFUN/USDC',
          base: 'HFUN',
          quote: 'USDC',
          active: true,
          isSpot: true,
          precision: { amount: 0.01, price: 0.0001 },
          limits: { amount: { min: 0.01, max: 100 }, leverage: { min: 1, max: 1 } },
          info: { hyperliquidId: 10001 },
        },
      ] as any,
    });

    const ws: any = (exchange as any).privateWebsocket;
    ws.applyOptimisticCorePositionFromFill(
      {
        coin: '@1',
        px: '18.5',
        sz: '3',
        side: 'B',
        dir: 'Buy',
        startPosition: '0',
        time: 1,
        oid: 1,
        hash: '0x0',
        crossed: true,
        fee: '0',
        tid: 1,
        closedPnl: '0',
      } as any,
      exchange.store.marketsById,
    );

    const positions = exchange.store.positions;
    exchange.dispose();

    expect(positions.some((p: any) => p.symbol === 'HFUN/USDC')).toBe(false);
  });

  it('returns empty rather than throwing when the spot response is malformed', async () => {
    const exchange = createExchange();
    (exchange as any).infoXhr = { post: async () => ({ data: [null, null] }) };

    const result = await (exchange as any).fetchSpotMarketsAndTickers();
    exchange.dispose();

    // Spot is best-effort: perps must still come up.
    expect(result).toEqual({ markets: [], tickers: [] });
  });

  it('never sends reduce-only on a spot ORDER WIRE, including TP/SL legs', async () => {
    // Asserts the emitted wire, not the helper. Testing `resolveReduceOnly`
    // directly is tautological: every `reduce_only:` call site could be
    // reverted to the pre-fix bug and a helper-level test would still pass.
    const exchange = createExchange();
    // Builder approval is an account fact, unrelated to the spot wire flags.
    (exchange as any)._builderFeeApproved = false;

    exchange.store.update({
      markets: [
        {
          id: '@1',
          symbol: 'HFUN/USDC',
          base: 'HFUN',
          quote: 'USDC',
          active: true,
          isSpot: true,
          precision: { amount: 0.01, price: 0.0001 },
          limits: { amount: { min: 0.01, max: 100 }, leverage: { min: 1, max: 1 } },
          info: { hyperliquidId: 10001 },
        },
      ] as any,
      tickers: [{ id: '@1', symbol: 'HFUN/USDC', last: 10, mark: 10, bid: 9.99, ask: 10.01 }] as any,
    });

    const posted: any[] = [];
    (exchange as any).xhr = {
      post: async (_url: string, body: any) => {
        posted.push(body);
        return { data: { status: 'ok', response: { type: 'order', data: { statuses: [{ resting: { oid: 1 } }] } } } };
      },
    };
    // Bypass EIP-712 signing; we only care about the action payload.
    (exchange as any).generateNonce = () => 1;

    await exchange
      .placeOrder({
        symbol: 'HFUN/USDC',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 1,
        price: 9,
        reduceOnly: true,
        stopLoss: 8,
        takeProfit: 12,
      } as any)
      .catch(() => undefined);

    exchange.dispose();

    const wires = posted.flatMap((b) => b?.action?.orders ?? []);
    expect(wires.length).toBeGreaterThan(0);
    // `r` is the reduce-only flag. Spot has no position to reduce, so every
    // wire — the entry AND the TP/SL legs that hardcode `true` on the perp
    // path — must send false.
    for (const wire of wires) {
      expect(wire.r).toBe(false);
      expect(wire.a).toBe(10001);
    }
  });

  it('no-ops leverage and margin-type changes on spot instead of signing a request', async () => {
    const exchange = createExchange();
    let posted = 0;
    (exchange as any).xhr = {
      post: async () => {
        posted += 1;
        return { data: {} };
      },
    };

    exchange.store.update({
      markets: [
        {
          id: '@1',
          symbol: 'HFUN/USDC',
          base: 'HFUN',
          quote: 'USDC',
          active: true,
          isSpot: true,
          precision: { amount: 0.01, price: 0.0001 },
          limits: { amount: { min: 0.01, max: 1 }, leverage: { min: 1, max: 1 } },
          info: { hyperliquidId: 10001 },
        },
      ] as any,
    });

    await exchange.setLeverage('HFUN/USDC', 5);
    await exchange.changeMarginType(true, 'HFUN/USDC');
    exchange.dispose();

    // Spot is always 1x cross; `updateLeverage` on a spot asset index is not
    // a meaningful action and must not be signed or sent.
    expect(posted).toBe(0);
  });

  it('caps spot prices at 8 decimals and perp prices at 6', () => {
    const exchange = createExchange();

    exchange.store.update({
      markets: [
        {
          id: '@1',
          symbol: 'HFUN/USDC',
          base: 'HFUN',
          quote: 'USDC',
          active: true,
          isSpot: true,
          precision: { amount: 0.01, price: 0.0001 },
          limits: { amount: { min: 0.01, max: 1 }, leverage: { min: 1, max: 1 } },
        },
        {
          id: 'BTC',
          symbol: 'BTC-PERP',
          base: 'BTC',
          quote: 'USDC',
          active: true,
          isSpot: false,
          precision: { amount: 0.01, price: 0.1 },
          limits: { amount: { min: 0.01, max: 1 }, leverage: { min: 1, max: 50 } },
        },
      ] as any,
    });

    // amount precision 0.01 => 2 decimals. Spot budget 8-2=6, perp 6-2=4.
    const spotPrice = exchange.priceToPrecision('HFUN/USDC', 0.000123456789);
    const perpPrice = exchange.priceToPrecision('BTC-PERP', 0.000123456789);
    exchange.dispose();

    const decimalsOf = (s: string) => (s.split('.')[1] ?? '').length;
    expect(decimalsOf(spotPrice)).toBeGreaterThan(decimalsOf(perpPrice));
    expect(decimalsOf(spotPrice)).toBeLessThanOrEqual(8);
    expect(decimalsOf(perpPrice)).toBeLessThanOrEqual(6);
  });
});
