import { describe, it, expect } from 'vitest';
import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { PositionSide, MarginType, PositionMode, type Position, type Balance } from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';

class TestParser extends TradingBaseCommand {
  readonly name = 'test-parser';
  readonly description = 'test harness';
  readonly category = CommandCategory.Trading;
  readonly aliases: string[] = [];
  readonly examples: string[] = [];
  async execute(_ctx: CommandContext): Promise<CommandResult> {
    return this.success();
  }
  // Expose the protected method for tests
  parse(
    exchange: ExchangeLike,
    symbol: string,
    size: string,
    allowPct: boolean,
    side?: PositionSide,
    scope?: 'position' | 'balance',
  ): number {
    return this.parseSizeAndConvertIfUsd(exchange, symbol, size, allowPct, side, scope);
  }
}

function pos(p: Partial<Position> = {}): Position {
  return {
    id: p.id ?? 'pos1',
    symbol: p.symbol ?? 'BTCUSDT',
    side: p.side ?? PositionSide.Long,
    entryPrice: p.entryPrice ?? 50_000,
    notional: p.notional ?? 50_000,
    leverage: p.leverage ?? 1,
    marginType: p.marginType ?? MarginType.Isolated,
    positionMode: p.positionMode ?? PositionMode.OneWay,
    unrealizedPnl: p.unrealizedPnl ?? 0,
    contracts: p.contracts ?? 1,
    liquidationPrice: p.liquidationPrice ?? 0,
  } as Position;
}

function bal(p: Partial<Balance> = {}): Balance {
  return {
    used: p.used ?? 0,
    free: p.free ?? 1000,
    total: p.total ?? 1000,
    upnl: p.upnl ?? 0,
  } as Balance;
}

function makeExchange(positions: Position[] = [], balance: Balance = bal()): ExchangeLike {
  return {
    // A REAL venue id, not 'test': every write now goes through the shared
    // order policy, which asks `exchangeMetadata` about round-down, infinity
    // stops and the reduce-only clamp. A made-up name answers "unknown venue"
    // and no longer exercises the branches these tests are about.
    name: 'binance',
    store: {
      markets: [
        { symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } },
      ],
      tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
      orders: [],
      positions,
      balance,
    },
    emitter: { on: () => {}, off: () => {}, emit: () => {} },
  } as unknown as ExchangeLike;
}

describe('size syntax', () => {
  const parser = new TestParser();

  describe('USD forms', () => {
    it('accepts $1000 prefix', () => {
      const ex = makeExchange();
      expect(parser.parse(ex, 'BTCUSDT', '$1000', false)).toBe(0.02);
    });

    it('accepts 1000$ suffix', () => {
      const ex = makeExchange();
      expect(parser.parse(ex, 'BTCUSDT', '1000$', false)).toBe(0.02);
    });

    it('accepts decimals on either side', () => {
      const ex = makeExchange();
      expect(parser.parse(ex, 'BTCUSDT', '$50.5', false)).toBeCloseTo(0.001);
      expect(parser.parse(ex, 'BTCUSDT', '50.5$', false)).toBeCloseTo(0.001);
    });

    it('rejects bare numbers', () => {
      const ex = makeExchange();
      expect(() => parser.parse(ex, 'BTCUSDT', '100', false)).toThrow(/Invalid size/);
    });

    it('rejects bare numbers even when % is allowed', () => {
      const ex = makeExchange([pos()]);
      expect(() => parser.parse(ex, 'BTCUSDT', '100', true)).toThrow(/Invalid size/);
    });

    it('rejects empty / garbage', () => {
      const ex = makeExchange();
      expect(() => parser.parse(ex, 'BTCUSDT', '', false)).toThrow(/Invalid size/);
      expect(() => parser.parse(ex, 'BTCUSDT', 'abc', false)).toThrow(/Invalid size/);
      expect(() => parser.parse(ex, 'BTCUSDT', '$', false)).toThrow();
    });
  });

  describe('% with default scope (legacy = position)', () => {
    it('% of position size', () => {
      const ex = makeExchange([pos({ contracts: 2 })]);
      expect(parser.parse(ex, 'BTCUSDT', '50%', true)).toBe(1);
    });

    it('throws when no position', () => {
      const ex = makeExchange();
      expect(() => parser.parse(ex, 'BTCUSDT', '50%', true)).toThrow(/No.*position/);
    });
  });

  describe('% with balance scope', () => {
    it('resolves against balance.free * leverage / price', () => {
      const ex = makeExchange([], bal({ free: 1000 })); // 1000 free, 1x leverage
      // 50% of 1000 USD = 500 USD, / 50000 price = 0.01
      expect(parser.parse(ex, 'BTCUSDT', '50%', true, undefined, 'balance')).toBe(0.01);
    });

    it('uses position leverage when one is open', () => {
      const ex = makeExchange(
        [pos({ leverage: 5, contracts: 0.1 })], // a position with 5x leverage
        bal({ free: 1000 }),
      );
      // 50% of (1000 * 5) = 2500 USD, / 50000 = 0.05
      expect(parser.parse(ex, 'BTCUSDT', '50%', true, undefined, 'balance')).toBe(0.05);
    });
  });

  describe('explicit %bal / %pos overrides', () => {
    it('%bal forces balance even when default is position', () => {
      const ex = makeExchange([pos({ contracts: 2 })], bal({ free: 1000 }));
      // %bal: 50% of 1000 / 50000 = 0.01
      expect(parser.parse(ex, 'BTCUSDT', '50%bal', true)).toBe(0.01);
    });

    it('%pos forces position even when default is balance', () => {
      const ex = makeExchange([pos({ contracts: 2 })], bal({ free: 1000 }));
      // %pos: 50% of 2 contracts = 1
      expect(parser.parse(ex, 'BTCUSDT', '50%pos', true, undefined, 'balance')).toBe(1);
    });
  });

  describe('% disallowed', () => {
    it('throws when allowPercentage is false', () => {
      const ex = makeExchange([pos()]);
      expect(() => parser.parse(ex, 'BTCUSDT', '50%', false)).toThrow(/Invalid size/);
    });
  });
});
