import { describe, expect, it } from 'vitest';

import type { Market, PlaceOrderOpts, Position } from '../types';
import { BaseExchange } from '../exchanges/base';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderType, PositionMode, PositionSide } from '../types';

class NukeTestExchange extends BaseExchange {
  readonly calls: string[] = [];
  readonly placed: PlaceOrderOpts[] = [];
  failCancel = false;

  constructor() {
    super('binance', { key: 'key', secret: 'secret' }, new DefaultStore());
  }

  placeOrder = async (opts: PlaceOrderOpts) => {
    this.calls.push(`place ${opts.symbol}`);
    this.placed.push(opts);
    return [] as string[];
  };

  cancelAllOrders = async () => {
    this.calls.push('cancel');
    if (this.failCancel) throw new Error('cancel endpoint down');
  };
}

const market = (symbol: string, contractSize: number): Market => ({
  id: symbol,
  symbol,
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.01, price: 0.1 },
  limits: { amount: { min: 0.01, max: 1_000_000 }, leverage: { min: 1, max: 100 } },
  contractSize,
});

const position = (overrides: Partial<Position>): Position => ({
  id: `${overrides.symbol}-long`,
  symbol: 'BTCUSDT',
  side: PositionSide.Long,
  entryPrice: 82_300,
  notional: 98.76,
  leverage: 1,
  positionMode: PositionMode.OneWay,
  unrealizedPnl: 0,
  realizedPnl: 0,
  contracts: 0.0012,
  liquidationPrice: 0,
  lastUpdate: 0,
  ...overrides,
});

const seed = (exchange: NukeTestExchange, markets: Market[], positions: Position[]) => {
  exchange.store.update({ markets, positions, loaded: { ...exchange.store.loaded, markets: true } });
};

describe('BaseExchange nuke sizing', () => {
  it('closes a linear contract-sized position in native contracts', async () => {
    const exchange = new NukeTestExchange();
    seed(exchange, [market('BTCUSDT', 0.01)], [position({ symbol: 'BTCUSDT', contracts: 0.0012 })]);

    await exchange.nukePositions(3);

    expect(exchange.placed).toHaveLength(1);
    expect(exchange.placed[0]).toMatchObject({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.Market,
      reduceOnly: true,
    });
    expect(exchange.placed[0].amount).toBeCloseTo(0.12, 10);
  });

  it('skips a position whose market is not loaded and still closes the rest', async () => {
    const exchange = new NukeTestExchange();
    seed(
      exchange,
      [market('ETHUSDT', 0.1)],
      [position({ symbol: 'BTCUSDT' }), position({ symbol: 'ETHUSDT', contracts: 0.5 })],
    );

    await exchange.nukePositions(3);

    expect(exchange.placed.map((o) => o.symbol)).toEqual(['ETHUSDT']);
    expect(exchange.placed[0].amount).toBeCloseTo(5, 10);
  });

  it('cancels resting orders before closing, and still closes when the cancel fails', async () => {
    const exchange = new NukeTestExchange();
    exchange.failCancel = true;
    seed(exchange, [market('BTCUSDT', 0.01)], [position({ symbol: 'BTCUSDT' })]);

    await exchange.nuke(3);

    expect(exchange.calls).toEqual(['cancel', 'place BTCUSDT']);
  });
});
