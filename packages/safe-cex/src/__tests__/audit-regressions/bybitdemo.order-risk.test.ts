import { describe, expect, it } from 'vitest';

import { BybitDemoExchange } from '../reference/bybit/bybitdemo.exchange';
import { DefaultStore } from '../../store/store.base';
import { Market, Position, PositionMode, PositionSide, Ticker } from '../../types';
import { calculateOrdersRisk } from '../../utils/risk';

const market = {
  id: 'ARBUSDT',
  symbol: 'ARBUSDT',
  base: 'ARB',
  quote: 'USDT',
  active: true,
  makerFee: 0,
  takerFee: 0,
  precision: {
    amount: 0.1,
    price: 0.00001,
  },
  limits: {
    amount: {
      min: 0.1,
      max: 1_000_000,
    },
    leverage: {
      min: 1,
      max: 50,
    },
  },
} as Market;

const ticker = {
  id: 'ARBUSDT',
  symbol: 'ARBUSDT',
  bid: 0.20499,
  ask: 0.20501,
  last: 0.205,
  mark: 0.205,
  index: 0.205,
  percentage: 0,
  openInterest: 0,
  fundingRate: 0,
  fundingTime: 0,
  volume: 0,
  quoteVolume: 0,
} as Ticker;

const position = {
  id: 'ARBUSDT',
  symbol: 'ARBUSDT',
  side: PositionSide.Long,
  entryPrice: 0.20515,
  notional: 41.03,
  leverage: 4,
  unrealizedPnl: -0.03,
  realizedPnl: 0,
  contracts: 200,
  liquidationPrice: 0,
  positionMode: PositionMode.OneWay,
} as Position;

const demoTradingStopOrder = (overrides: Record<string, unknown>) => ({
  orderId: 'tp-1',
  orderStatus: 'Untriggered',
  symbol: 'ARBUSDT',
  // Demo snapshots can omit the protective side on position TP/SL rows.
  side: '',
  orderType: 'Market',
  stopOrderType: 'UNKNOWN',
  createType: 'CreateByTakeProfit',
  tpslMode: 'Full',
  triggerPrice: '0.20545',
  price: '',
  qty: '0',
  leavesQty: '0',
  cumExecQty: '0',
  reduceOnly: true,
  closeOnTrigger: true,
  timeInForce: 'GTC',
  triggerBy: 'MarkPrice',
  positionIdx: 0,
  ...overrides,
});

describe('Bybit Demo order risk', () => {
  it('estimates realized and unrealized PnL for side-less demo position TP/SL rows', () => {
    const store = new DefaultStore();
    store.update({
      markets: [market],
      tickers: [ticker],
      positions: [position],
    });

    const exchange = new BybitDemoExchange('bybitdemo', { key: 'key', secret: 'secret' }, store);
    const [takeProfit] = exchange.mapOrder(demoTradingStopOrder({}));
    const [stopLoss] = exchange.mapOrder(
      demoTradingStopOrder({
        orderId: 'sl-1',
        createType: 'CreateByStopLoss',
        triggerPrice: '0.20485',
      }),
    );

    const ordersRisk = calculateOrdersRisk(store.markets, store.tickers, store.positions, [takeProfit, stopLoss]);

    expect(takeProfit.side).toBe('sell');
    expect(stopLoss.side).toBe('sell');
    expect(ordersRisk[takeProfit.id][0].orderPnl).toBeCloseTo(0.06);
    expect(ordersRisk[takeProfit.id][0].rollingUnrealizedPnl).toBeCloseTo(0.06);
    expect(ordersRisk[stopLoss.id][0].orderPnl).toBeCloseTo(-0.06);
    expect(ordersRisk[stopLoss.id][0].rollingUnrealizedPnl).toBeCloseTo(-0.06);
  });
});
