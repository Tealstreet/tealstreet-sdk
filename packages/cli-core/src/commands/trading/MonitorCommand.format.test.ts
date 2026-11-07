import { describe, expect, it, vi } from 'vitest';

import {
  OrderStatus,
  OrderType,
  PositionSide,
  type Market,
  type Order,
  type Position,
} from '@tealstreet/safe-cex/src/types';
import { DefaultStore } from '@tealstreet/safe-cex/src/store/store.base';
import {
  formatMarginTable,
  formatMultiAccountOrdersTable,
  formatMultiAccountPositionsTable,
  formatOrdersTable,
  formatPositionsTable,
} from './MonitorCommand';
import type { CLIPlatformAdapter, ExchangeLike } from '../../types';
import { OrdersCommand } from './OrdersCommand';
import { PositionsCommand } from './PositionsCommand';

// `formatPrice` and `formatPriceCompact` both PREFIX the currency symbol. Every
// call site that also wrote a literal `$` rendered `$$0.00`, which a user hit on
// the shipped 0.10.5 binary. This pins the symbol count, not the numbers.
const exchangeWith = (balance: { total: number; free: number; used: number; upnl: number }): ExchangeLike =>
  ({
    name: 'bybitdemo',
    store: { balance, positions: [] },
    emitter: { on: () => {}, off: () => {}, emit: () => {} },
  }) as unknown as ExchangeLike;

describe('margin table currency rendering', () => {
  it('never doubles the currency symbol', () => {
    const table = formatMarginTable(exchangeWith({ total: 0, free: 0, used: 0, upnl: 0 }));
    expect(table).not.toMatch(/\$\$/);
  });

  it('still renders one symbol per money value', () => {
    const table = formatMarginTable(exchangeWith({ total: 1234.5, free: 1000, used: 234.5, upnl: 12.25 }));
    expect(table).toMatch(/\$1,234\.50/);
    expect(table.match(/\$/g)?.length).toBe(5);
  });

  it('leaves the non-money rows without a symbol', () => {
    const table = formatMarginTable(exchangeWith({ total: 1000, free: 1000, used: 0, upnl: 0 }));
    const levRow = table.split('\n').find((line) => line.includes('Account Lev')) ?? '';
    const ratioRow = table.split('\n').find((line) => line.includes('Margin Ratio')) ?? '';
    expect(levRow).not.toContain('$');
    expect(ratioRow).not.toContain('$');
  });
});

const cells = (s: string): string[] =>
  s
    .replace(/\x1b\[[0-9;]*m/g, '')
    .split(/[│\n]/)
    .map((c) => c.trim());

const market = (amount: number, contractSize: number, base: string) =>
  ({ symbol: `${base}USDT`, base, precision: { amount, price: 0.00001 }, contractSize }) as Market;
const doge = market(0.01, 1000, 'DOGE');

describe('size column', () => {
  it.each([false, true])('retains DefaultStore market precision in ps (all=%s)', async (all) => {
    const m = market(0.000001, 1, 'BTC');
    const position = {
      symbol: m.symbol, side: PositionSide.Long, contracts: 0.000006, entryPrice: 100000,
      notional: 0.6, unrealizedPnl: 0, leverage: 1,
    } as Position;
    const store = new DefaultStore();
    store.update({ markets: [m], positions: [position] });
    const exchange = { store } as ExchangeLike;
    const print = vi.fn();
    const adapter = { print } as unknown as CLIPlatformAdapter;
    expect(cells(formatPositionsTable(exchange))).toContain('0.000006');

    await new PositionsCommand().execute({ raw: 'ps', args: { _: ['ps'], all }, symbol: m.symbol, exchange, adapter });

    expect(cells(print.mock.calls.flat(2).join('\n'))).toContain('0.000006');
  });

  it('keeps tiny inverse base sizes visible in order and position tables', () => {
    const m = { ...market(1, 1, 'BTC'), isInverse: true };
    const position = { symbol: m.symbol, side: PositionSide.Long, contracts: 0.00001, entryPrice: 100000, notional: 1 };
    const order = { id: 'inverse', symbol: m.symbol, status: OrderStatus.Open, type: OrderType.Limit, amount: 1, remaining: 0.00001 };
    const store = new DefaultStore();
    store.update({ markets: [m], positions: [position as Position], orders: [order as Order] });
    const exchange = { store } as ExchangeLike;

    expect(cells(formatPositionsTable(exchange))).toContain('0.00001');
    expect(cells(formatOrdersTable(exchange))).toContain('0.00001');
    expect(cells(formatMultiAccountPositionsTable([{ accountName: 'acct', exchange }]))).toContain('0.00001');
    expect(cells(formatMultiAccountOrdersTable([{ accountName: 'acct', exchange }]))).toContain('0.00001');
  });

  // OKX reports `amount` in contracts and `remaining` in base.
  it.each([
    { name: 'DOGE', market: doge, amount: 0.4, remaining: 400, expected: '400' },
    { name: 'BTC', market: market(0.01, 0.01, 'BTC'), amount: 0.12, remaining: 0.0012, expected: '0.0012' },
  ])('shows OKX $name orders in base units in o and both orders tables', async ({ market: m, expected, ...size }) => {
    const order = { id: 'o1', status: OrderStatus.Open, symbol: m.symbol, type: OrderType.Limit, price: 0.1, ...size };
    const store = new DefaultStore();
    store.update({ markets: [m], orders: [order as Order] });
    const exchange = { store } as ExchangeLike;
    const print = vi.fn();
    const adapter = { print } as unknown as CLIPlatformAdapter;
    await new OrdersCommand().execute({ raw: 'o', args: { _: ['o'] }, symbol: m.symbol, exchange, adapter });

    expect(cells(formatOrdersTable(exchange))).toContain(expected);
    expect(cells(formatMultiAccountOrdersTable([{ accountName: 'acct', exchange }]))).toContain(expected);
    expect(cells(print.mock.calls.flat(2).join('\n'))).toContain(expected);
  });

  it('shows OKX DOGE positions in base units in positions --all and both positions tables', async () => {
    const position = { symbol: doge.symbol, side: PositionSide.Long, contracts: 240, entryPrice: 0.1, notional: 24 };
    const exchange = { store: { markets: [doge], positions: [position as Position], orders: [] } } as ExchangeLike;
    const print = vi.fn();
    const adapter = { print } as unknown as CLIPlatformAdapter;
    await new PositionsCommand().execute({ raw: 'ps', args: { _: ['ps'], all: true }, exchange, adapter });

    expect(cells(formatPositionsTable(exchange))).toContain('240');
    expect(cells(formatMultiAccountPositionsTable([{ accountName: 'acct', exchange }]))).toContain('240');
    expect(cells(print.mock.calls.flat(2).join('\n'))).toContain('240');
  });
});
