import type {
  ActiveChaser,
  Balance,
  Candle,
  ExchangeName,
  Market,
  Order,
  OrderBook,
  PlaceOrderOpts,
  Position,
  StoreDataLoaded,
  Ticker,
} from './types';

import { describe, expect, it } from 'vitest';

import {
  LogSeverity,
  MarginType,
  OrderRejectReason,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
  timeframeToSeconds,
} from './types';

describe('types', () => {
  describe('OrderStatus enum', () => {
    it('should have Open status', () => {
      expect(OrderStatus.Open).toBe('open');
    });

    it('should have Closed status', () => {
      expect(OrderStatus.Closed).toBe('closed');
    });

    it('should have Canceled status', () => {
      expect(OrderStatus.Canceled).toBe('canceled');
    });

    it('should have local Creating status', () => {
      expect(OrderStatus.Creating).toBe('creating');
    });

    it('should have local Updating status', () => {
      expect(OrderStatus.Updating).toBe('updating');
    });

    it('should have local Canceling status', () => {
      expect(OrderStatus.Canceling).toBe('canceling');
    });
  });

  describe('OrderType enum', () => {
    it('should have Market type', () => {
      expect(OrderType.Market).toBe('market');
    });

    it('should have Limit type', () => {
      expect(OrderType.Limit).toBe('limit');
    });

    it('should have Conditional type', () => {
      expect(OrderType.Conditional).toBe('conditional');
    });

    it('should have StopLoss type', () => {
      expect(OrderType.StopLoss).toBe('stop_market');
    });

    it('should have TakeProfit type', () => {
      expect(OrderType.TakeProfit).toBe('take_profit_market');
    });

    it('should have TrailingStopLoss type', () => {
      expect(OrderType.TrailingStopLoss).toBe('trailing_stop_market');
    });
  });

  describe('OrderSide enum', () => {
    it('should have Buy side', () => {
      expect(OrderSide.Buy).toBe('buy');
    });

    it('should have Sell side', () => {
      expect(OrderSide.Sell).toBe('sell');
    });
  });

  describe('OrderTimeInForce enum', () => {
    it('should have GoodTillCancel', () => {
      expect(OrderTimeInForce.GoodTillCancel).toBe('GoodTillCancel');
    });

    it('should have ImmediateOrCancel', () => {
      expect(OrderTimeInForce.ImmediateOrCancel).toBe('ImmediateOrCancel');
    });

    it('should have FillOrKill', () => {
      expect(OrderTimeInForce.FillOrKill).toBe('FillOrKill');
    });

    it('should have PostOnly', () => {
      expect(OrderTimeInForce.PostOnly).toBe('PostOnly');
    });
  });

  describe('OrderStopTriggerType enum', () => {
    it('should have MarkPrice', () => {
      expect(OrderStopTriggerType.MarkPrice).toBe('mark_price');
    });

    it('should have LastPrice', () => {
      expect(OrderStopTriggerType.LastPrice).toBe('last_price');
    });

    it('should have IndexPrice', () => {
      expect(OrderStopTriggerType.IndexPrice).toBe('index_price');
    });
  });

  describe('OrderRejectReason enum', () => {
    it('should have PostOnly reason', () => {
      expect(OrderRejectReason.PostOnly).toBe('PostOnly');
    });

    it('should have InsufficientBalance reason', () => {
      expect(OrderRejectReason.InsufficientBalance).toBe('InsufficientBalance');
    });

    it('should have ReduceOnly reason', () => {
      expect(OrderRejectReason.ReduceOnly).toBe('ReduceOnly');
    });
  });

  describe('PositionSide enum', () => {
    it('should have Long side', () => {
      expect(PositionSide.Long).toBe('long');
    });

    it('should have Short side', () => {
      expect(PositionSide.Short).toBe('short');
    });
  });

  describe('PositionMode enum', () => {
    it('should have Hedged mode', () => {
      expect(PositionMode.Hedged).toBe('hedged');
    });

    it('should have OneWay mode', () => {
      expect(PositionMode.OneWay).toBe('oneway');
    });
  });

  describe('MarginType enum', () => {
    it('should have Isolated type', () => {
      expect(MarginType.Isolated).toBe('isolated');
    });

    it('should have Cross type', () => {
      expect(MarginType.Cross).toBe('cross');
    });
  });

  describe('LogSeverity enum', () => {
    it('should have Debug level', () => {
      expect(LogSeverity.Debug).toBe('debug');
    });

    it('should have Info level', () => {
      expect(LogSeverity.Info).toBe('info');
    });

    it('should have Warning level', () => {
      expect(LogSeverity.Warning).toBe('warning');
    });

    it('should have Error level', () => {
      expect(LogSeverity.Error).toBe('error');
    });
  });

  describe('timeframeToSeconds', () => {
    describe('second timeframes', () => {
      it('should have 1s = 1', () => {
        expect(timeframeToSeconds['1s']).toBe(1);
      });

      it('should have 5s = 5', () => {
        expect(timeframeToSeconds['5s']).toBe(5);
      });

      it('should have 15s = 15', () => {
        expect(timeframeToSeconds['15s']).toBe(15);
      });

      it('should have 30s = 30', () => {
        expect(timeframeToSeconds['30s']).toBe(30);
      });
    });

    describe('minute timeframes', () => {
      it('should have 1m = 60', () => {
        expect(timeframeToSeconds['1m']).toBe(60);
      });

      it('should have 3m = 180', () => {
        expect(timeframeToSeconds['3m']).toBe(180);
      });

      it('should have 5m = 300', () => {
        expect(timeframeToSeconds['5m']).toBe(300);
      });

      it('should have 15m = 900', () => {
        expect(timeframeToSeconds['15m']).toBe(900);
      });

      it('should have 30m = 1800', () => {
        expect(timeframeToSeconds['30m']).toBe(1800);
      });
    });

    describe('hour timeframes', () => {
      it('should have 1h = 3600', () => {
        expect(timeframeToSeconds['1h']).toBe(3600);
      });

      it('should have 2h = 7200', () => {
        expect(timeframeToSeconds['2h']).toBe(7200);
      });

      it('should have 4h = 14400', () => {
        expect(timeframeToSeconds['4h']).toBe(14400);
      });

      it('should have 6h = 21600', () => {
        expect(timeframeToSeconds['6h']).toBe(21600);
      });

      it('should have 12h = 43200', () => {
        expect(timeframeToSeconds['12h']).toBe(43200);
      });
    });

    describe('day timeframes', () => {
      it('should have 1d = 86400', () => {
        expect(timeframeToSeconds['1d']).toBe(86400);
      });

      it('should have 24h = 86400', () => {
        expect(timeframeToSeconds['24h']).toBe(86400);
      });

      it('should have 1w = 604800', () => {
        expect(timeframeToSeconds['1w']).toBe(604800);
      });

      it('should have 1M = 2592000', () => {
        expect(timeframeToSeconds['1M']).toBe(2592000);
      });
    });

    describe('extended timeframes', () => {
      it('should have 3h = 10800', () => {
        expect(timeframeToSeconds['3h']).toBe(10800);
      });

      it('should have 8h = 28800', () => {
        expect(timeframeToSeconds['8h']).toBe(28800);
      });

      it('should have 2d = 172800', () => {
        expect(timeframeToSeconds['2d']).toBe(172800);
      });
    });
  });

  describe('type structures', () => {
    it('should allow creating a Balance object', () => {
      const balance: Balance = {
        used: 100,
        free: 900,
        total: 1000,
        upnl: 50,
      };

      expect(balance.total).toBe(balance.used + balance.free);
    });

    it('should allow creating a Market object', () => {
      const market: Market = {
        id: 'btcusdt',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: {
          amount: 3,
          price: 2,
        },
        limits: {
          amount: { min: 0.001, max: 100 },
          leverage: { min: 1, max: 125 },
        },
      };

      expect(market.symbol).toBe('BTCUSDT');
      expect(market.precision.amount).toBe(3);
    });

    it('should allow creating a Ticker object', () => {
      const ticker: Ticker = {
        id: 'btcusdt',
        symbol: 'BTCUSDT',
        bid: 50000,
        ask: 50001,
        last: 50000.5,
        mark: 50000.5,
        index: 50000.5,
        percentage: 1.5,
        openInterest: 1000000,
        fundingRate: 0.0001,
        fundingTime: Date.now(),
        volume: 10000,
        quoteVolume: 500000000,
      };

      expect(ticker.last).toBe(50000.5);
    });

    it('should allow creating an Order object', () => {
      const order: Order = {
        id: 'order-123',
        status: OrderStatus.Open,
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        price: 50000,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: false,
        lastUpdate: Date.now(),
      };

      expect(order.status).toBe(OrderStatus.Open);
      expect(order.remaining).toBe(order.amount - order.filled);
    });

    it('should allow creating a Position object', () => {
      const position: Position = {
        id: 'pos-123',
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        entryPrice: 50000,
        notional: 5000,
        leverage: 10,
        unrealizedPnl: 100,
        contracts: 0.1,
        liquidationPrice: 45000,
      };

      expect(position.side).toBe(PositionSide.Long);
      expect(position.leverage).toBe(10);
    });

    it('should allow creating a Candle object', () => {
      const candle: Candle = {
        timestamp: Date.now(),
        open: 50000,
        high: 50500,
        low: 49500,
        close: 50200,
        volume: 100,
      };

      expect(candle.high).toBeGreaterThan(candle.low);
    });

    it('should allow creating PlaceOrderOpts', () => {
      const opts: PlaceOrderOpts = {
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 50000,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      };

      expect(opts.type).toBe(OrderType.Limit);
    });

    it('should allow creating an OrderBook object', () => {
      const orderBook: OrderBook = {
        bids: [
          { price: 50000, amount: 1.0, total: 1.0 },
          { price: 49999, amount: 0.5, total: 1.5 },
        ],
        asks: [
          { price: 50001, amount: 0.8, total: 0.8 },
          { price: 50002, amount: 1.2, total: 2.0 },
        ],
      };

      expect(orderBook.bids[0].price).toBeGreaterThan(orderBook.bids[1].price);
      expect(orderBook.asks[0].price).toBeLessThan(orderBook.asks[1].price);
    });

    it('should allow creating StoreDataLoaded object', () => {
      const loaded: StoreDataLoaded = {
        balance: true,
        orders: true,
        markets: true,
        tickers: false,
        positions: true,
      };

      expect(loaded.tickers).toBe(false);
    });

    it('should allow creating ActiveChaser object', () => {
      const chaser: ActiveChaser = {
        id: 'chaser-123',
        symbol: 'ETHUSDT',
        size: 1.0,
        side: OrderSide.Buy,
        maxAttempts: 10,
        attempts: 3,
        isStopped: false,
        stopReason: '',
        origin: 'web',
      };

      expect(chaser.attempts).toBeLessThan(chaser.maxAttempts);
    });
  });

  describe('ExchangeName type', () => {
    it('should accept valid exchange names', () => {
      const exchanges: ExchangeName[] = [
        'binance',
        'bybit',
        'okx',
        'hyperliquid',
        'bitmex',
        'bitmextestnet',
        'coinbase',
        'kraken',
        'weex',
      ];

      expect(exchanges).toHaveLength(9);
      exchanges.forEach((exchange) => {
        expect(typeof exchange).toBe('string');
      });
    });
  });
});
