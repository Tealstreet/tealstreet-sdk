import { describe, expect, it, vi } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { OrderSide, OrderType } from '../../types';
import { applyReferenceFixture } from '../cutover/referenceFixture';
import { HyperliquidExchange } from '../reference/hyperliquid/hyperliquid.exchange';
import { HyperliquidSpotExchange } from '../reference/hyperliquid/hyperliquidspot.exchange';

// AGT-117: a market entry was sent alone (`na`) and its stop-loss/take-profit dropped, while
// the result named the entry as placed. It now carries them under `normalTpsl` as a limit
// entry does. This wire was accepted live on 2026-11-21 (DOGE): the venue answered a filled
// parent followed by two `waitingForTrigger` legs, both resting as reduce-only triggers.
// Built at runtime: a 0x+64-hex literal trips gitleaks' private-key rule on the public mirror.
const SECRET = `0x${'11'.repeat(32)}`;
const WALLET = '0x0000000000000000000000000000000000000001';

const okResponse = (statuses: unknown[]) => ({
  data: { status: 'ok', response: { type: 'order', data: { statuses } } },
});
const FILLED = { filled: { oid: 1, totalSz: '0.1', avgPx: '78500' } };

const createPerp = () => {
  const store = new FrameStore();
  applyReferenceFixture(
    store,
    {
      markets: [
        {
          id: 'BTC',
          symbol: 'BTC-PERP',
          base: 'BTC',
          quote: 'USDC',
          active: true,
          precision: { amount: 0.00001, price: 1 },
          limits: { amount: { min: 0.00001, max: 1000 }, leverage: { min: 1, max: 100 } },
          contractSize: 1,
          isSpot: false,
          info: { hyperliquidId: 0 },
        } as any,
      ],
      tickers: [
        { id: 'BTC', symbol: 'BTC-PERP', bid: 78490, ask: 78510, last: 78500, mark: 78500, index: 78502 } as any,
      ],
    },
    store.markets.map((row) => row.symbol),
  );
  const exchange = new HyperliquidExchange('hyperliquid' as any, { key: WALLET, secret: SECRET } as any, store);
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  return exchange;
};

const actionOf = (post: ReturnType<typeof vi.fn>) => post.mock.calls[0][1].action;

describe('hyperliquid market entries attach their SL/TP under normalTpsl', () => {
  it.each([
    [OrderSide.Buy, '82425', false],
    [OrderSide.Sell, '74575', true],
  ])('%s market entry with SL and TP', async (side, parentPx, legIsBuy) => {
    const exchange = createPerp();
    const post = vi.fn().mockResolvedValue(okResponse([FILLED, 'waitingForTrigger', 'waitingForTrigger']));
    exchange.xhr = { post } as any;

    const ids = await exchange.placeOrder({
      symbol: 'BTC-PERP',
      type: OrderType.Market,
      side,
      amount: 0.1,
      stopLoss: 70000,
      takeProfit: 90000,
    });

    // The legs carry no native ID in the reply, so the result is the entry alone.
    expect(ids).toEqual(['1']);
    const action = actionOf(post);
    expect(action.grouping).toBe('normalTpsl');
    const [entry, ...legs] = action.orders;
    expect(entry).toMatchObject({ a: 0, b: side === OrderSide.Buy, p: parentPx, s: '0.1', r: false });
    expect(entry.t).toEqual({ limit: { tif: 'Gtc' } });
    expect(legs).toHaveLength(2);
    legs.forEach((leg: any, index: number) => {
      expect(leg).toMatchObject({ a: 0, b: legIsBuy, p: parentPx, s: '0.1', r: true });
      expect(leg.t).toEqual({
        trigger: { isMarket: true, triggerPx: index === 0 ? '70000' : '90000', tpsl: index === 0 ? 'sl' : 'tp' },
      });
    });
    expect(new Set(action.orders.map((wire: any) => wire.c)).size).toBe(3);
  });

  it('a zero or absent leg price adds no leg; no legs stays a plain order', async () => {
    for (const [legs, grouping, kinds] of [
      [{ stopLoss: 0, takeProfit: 90000 }, 'normalTpsl', ['tp']],
      [{ stopLoss: 70000 }, 'normalTpsl', ['sl']],
      [{ stopLoss: 0, takeProfit: 0 }, 'na', []],
      [{}, 'na', []],
    ] as const) {
      const exchange = createPerp();
      const post = vi.fn().mockResolvedValue(okResponse([FILLED, ...kinds.map(() => 'waitingForTrigger')]));
      exchange.xhr = { post } as any;

      await exchange.placeOrder({
        symbol: 'BTC-PERP',
        type: OrderType.Market,
        side: OrderSide.Buy,
        amount: 0.1,
        ...legs,
      });

      const action = actionOf(post);
      expect(action.grouping).toBe(grouping);
      expect(action.orders.slice(1).map((wire: any) => wire.t.trigger.tpsl)).toEqual(kinds);
    }
  });

  it('spot market entry groups its legs without reduce-only', async () => {
    const store = new FrameStore();
    applyReferenceFixture(
      store,
      {
        markets: [
          {
            id: '@1',
            symbol: 'HFUN/USDC',
            base: 'HFUN',
            quote: 'USDC',
            active: true,
            isSpot: true,
            precision: { amount: 0.01, price: 0.0001 },
            limits: { amount: { min: 0.01, max: 1000000 }, leverage: { min: 1, max: 100 } },
            info: { hyperliquidId: 10001 },
          } as any,
        ],
        tickers: [{ id: '@1', symbol: 'HFUN/USDC', bid: 9.99, ask: 10.01, last: 10, mark: 10, index: 10 } as any],
      },
      store.markets.map((row) => row.symbol),
    );
    const exchange = new HyperliquidSpotExchange(
      'hyperliquidspot' as any,
      { key: WALLET, secret: SECRET } as any,
      store,
    );
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
    const post = vi.fn().mockResolvedValue(okResponse([FILLED, 'waitingForTrigger']));
    exchange.xhr = { post } as any;
    (exchange as any).requestSpotBalanceRefresh = vi.fn();

    await exchange.placeOrder({
      symbol: 'HFUN/USDC',
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 1,
      stopLoss: 9,
    });

    const action = actionOf(post);
    expect(action.grouping).toBe('normalTpsl');
    expect(action.orders).toHaveLength(2);
    expect(action.orders[1]).toMatchObject({ b: false, p: action.orders[0].p, s: '1', r: false });
    expect(action.orders[1].t.trigger).toMatchObject({ isMarket: true, tpsl: 'sl', triggerPx: '9' });
  });
});
