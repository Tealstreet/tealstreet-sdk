import type { Balance, Market, Order, Position, Ticker } from '../../types';

import { afterEach, describe, expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { BaseExchange } from '../../exchanges/base';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import {
  MarginType,
  OrderRejectReason,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';
import { consumerFrame } from './consumerFrame';

// These fixtures prove the binary adapter boundary independently of the Rust
// writer. Actual venue startup/parser coverage is in realVenue.wasm.test.ts.
const exchanges: BaseExchange[] = [];
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

function pair() {
  const options = { key: '', secret: '', publicDataOnly: true };
  const worker = new RustEngineExchange('binance', options, new DefaultStore());
  const main = new BaseExchange('binance', options, new DefaultStore());
  exchanges.push(worker, main);
  worker.onFrame = (buffer) => main.applyWorkerFrame(structuredClone(buffer, { transfer: [buffer] }));
  return {
    stores: [worker.store, main.store],
    deliver: (buffer: ArrayBuffer) =>
      (worker as unknown as { receiveFrame(buffer: ArrayBuffer): void }).receiveFrame(buffer),
  };
}

const market = (symbol: string, flags: Partial<Market> = {}): Market => ({
  id: `native-${symbol}`,
  symbol,
  base: symbol.startsWith('BTC') ? 'BTC' : 'ETH',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
  contractSize: 1,
  ...flags,
});
const ticker = (symbol: string, last: number): Ticker => ({
  id: `native-${symbol}`,
  symbol,
  bid: last - 1,
  ask: last + 1,
  last,
  mark: last,
  index: last,
  percentage: 1,
  openInterest: 10,
  fundingRate: 0.001,
  fundingTime: 1_700_000_000_000,
  volume: 20,
  quoteVolume: 2000,
});
const order = (id: string, symbol: string, changes: Partial<Order> = {}): Order => ({
  id,
  symbol,
  type: OrderType.Limit,
  status: OrderStatus.Open,
  side: OrderSide.Buy,
  price: 100,
  amount: 2,
  filled: 0,
  remaining: 2,
  reduceOnly: false,
  lastUpdate: 1_700_000_000_000,
  ...changes,
});
const position = (id: string, symbol: string, changes: Partial<Position> = {}): Position => ({
  id,
  symbol,
  side: PositionSide.Long,
  contracts: 2,
  entryPrice: 90,
  notional: 200,
  leverage: 10,
  unrealizedPnl: 20,
  liquidationPrice: 50,
  ...changes,
});
const balance: Balance = { used: 100, free: 900, total: 1000, upnl: 3 };

describe('independent frame metadata and market identities', () => {
  it('maps reordered multi-market rows and later sections to their actual symbol indices', () => {
    const { deliver, stores } = pair();
    const markets = [
      market('ETHUSDT', { isExpiring: true, isReadOnly: true }),
      market('BTCUSDT', { isPerpetual: true }),
    ];
    deliver(
      consumerFrame({
        markets,
        tickers: [ticker('BTCUSDT', 100), ticker('ETHUSDT', 200)],
        orders: [order('btc-order', 'BTCUSDT'), order('eth-order', 'ETHUSDT')],
        positions: [
          position('btc-cross', 'BTCUSDT', { marginType: MarginType.Cross, positionMode: PositionMode.Hedged }),
          position('btc-isolated', 'BTCUSDT', { marginType: MarginType.Isolated, positionMode: PositionMode.Hedged }),
        ],
        loaded: { markets: true, tickers: true, positions: true, balance: true, orders: true },
        options: { isHedged: true },
        fetchedLeverage: {
          BTCUSDT: { buyLeverage: 10, sellLeverage: 15, marginType: MarginType.Cross },
          ETHUSDT: { buyLeverage: 4, sellLeverage: 7, marginType: MarginType.Isolated },
        },
      }),
    );
    for (const store of stores) {
      expect(store.markets[0]).toMatchObject({ symbol: 'ETHUSDT', isExpiring: true, isReadOnly: true });
      expect(store.markets[1]).toMatchObject({ symbol: 'BTCUSDT', isPerpetual: true });
      expect(store.markets[1].isExpiring).toBeUndefined();
      expect(store.tickers.map((row) => [row.symbol, row.last])).toEqual([
        ['BTCUSDT', 100],
        ['ETHUSDT', 200],
      ]);
      expect(store.orders.map((row) => [row.id, row.symbol])).toEqual([
        ['btc-order', 'BTCUSDT'],
        ['eth-order', 'ETHUSDT'],
      ]);
      expect(store.positions.map((row) => [row.id, row.symbol, row.marginType])).toEqual([
        ['btc-cross', 'BTCUSDT', MarginType.Cross],
        ['btc-isolated', 'BTCUSDT', MarginType.Isolated],
      ]);
      expect(store.fetchedLeverage).toEqual({
        BTCUSDT: { buyLeverage: 10, sellLeverage: 15, marginType: MarginType.Cross, positionMode: PositionMode.Hedged },
        ETHUSDT: {
          buyLeverage: 4,
          sellLeverage: 7,
          marginType: MarginType.Isolated,
          positionMode: PositionMode.Hedged,
        },
      });
    }
    deliver(consumerFrame({ orders: [order('later-eth', 'ETHUSDT')] }, undefined, false, ['ETHUSDT', 'BTCUSDT']));
    // Delta indices refer to the ticker table (BTC first), not market order.
    deliver(consumerFrame({}, [1, 4, 250]));
    for (const store of stores) {
      expect(store.orders[0].symbol).toBe('ETHUSDT');
      expect(store.tickersBySymbol.ETHUSDT.last).toBe(250);
      expect(store.tickersBySymbol.BTCUSDT.last).toBe(100);
    }
  });

  it('retains explicit close values, trigger rails, rejection reasons and caller/parent IDs', () => {
    const { deliver, stores } = pair();
    const rows = [
      order('tp', 'BTCUSDT', {
        type: OrderType.TakeProfit,
        side: OrderSide.Sell,
        reduceOnly: true,
        close: true,
        tpTriggerType: OrderStopTriggerType.MarkPrice,
        takeProfit: 110,
        timeInForce: OrderTimeInForce.ImmediateOrCancel,
        clientOrderId: 'caller-42',
        parentId: 'entry',
        extra: { rejectReason: OrderRejectReason.ReduceOnly },
      }),
      order('sl', 'ETHUSDT', {
        type: OrderType.StopLoss,
        close: false,
        slTriggerType: OrderStopTriggerType.LastPrice,
        stopLoss: 80,
        extra: { rejectReason: OrderRejectReason.InsufficientBalance },
      }),
      order('trailing', 'BTCUSDT', {
        type: OrderType.TrailingStopLoss,
        slTriggerType: OrderStopTriggerType.IndexPrice,
        extra: { rejectReason: OrderRejectReason.PostOnly },
      }),
      order('plain', 'ETHUSDT'),
    ];
    deliver(consumerFrame({ markets: [market('BTCUSDT'), market('ETHUSDT')], orders: rows }));
    for (const store of stores) {
      rows.forEach((row, index) => expect(store.orders[index]).toMatchObject(row));
      expect(store.orders[0].slTriggerType).toBeUndefined();
      expect(store.orders[1].tpTriggerType).toBeUndefined();
      expect(store.orders[3]).not.toHaveProperty('close');
      expect(store.orders[3].extra).toBeUndefined();
    }
  });

  it.each([
    ['legacy scalar', balance],
    ['coin collateral', { ...balance, coins: { BTC: { base: 'BTC', amount: 0.5, notional: 50 } } }],
    [
      'currency balances without coins',
      { ...balance, currencies: { USDT: balance, BTC: { total: 0.5, free: 0.3, used: 0.2, upnl: 0.01 } } },
    ],
    [
      'both extensions',
      { ...balance, coins: { BTC: { base: 'BTC', amount: 0.5, notional: 50 } }, currencies: { USDT: balance } },
    ],
    ['explicit empty coins', { ...balance, coins: {} }],
    ['explicit empty currencies', { ...balance, currencies: {} }],
    ['both empty', { ...balance, coins: {}, currencies: {} }],
  ] as const)('distinguishes optional balance extensions: %s', (_label, value) => {
    const { deliver, stores } = pair();
    const buffer = consumerFrame({ balance: value });
    expect(new Frame(buffer).balance()).toEqual(value);
    deliver(buffer);
    for (const store of stores) expect(store.balance).toEqual(value);
  });

  it('clears explicitly empty balance extensions while a legacy scalar update preserves unknown details', () => {
    const { deliver, stores } = pair();
    const currencies = { USDT: balance };
    const coins = { BTC: { base: 'BTC', amount: 0.5, notional: 50 } };
    deliver(consumerFrame({ balance: { ...balance, coins, currencies } }));
    deliver(consumerFrame({ balance: { ...balance, total: 1100 } }));
    for (const store of stores) expect(store.balance).toEqual({ ...balance, total: 1100, coins, currencies });
    deliver(consumerFrame({ balance: { ...balance, coins: {}, currencies: {} } }));
    for (const store of stores) expect(store.balance).toEqual({ ...balance, coins: {}, currencies: {} });
  });
});
