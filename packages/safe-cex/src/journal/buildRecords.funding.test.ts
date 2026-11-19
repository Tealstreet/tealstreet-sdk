import type { Execution } from '../types';

import { describe, expect, it } from 'vitest';

import { OrderSide, PositionSide } from '../types';
import { buildFoldedPositionRecords } from './buildRecords';

const FROM = 1_700_000_000_000;
const symbol = 'BTCUSDT';
const fills: Execution[] = [
  {
    id: '101',
    symbol,
    timestamp: FROM / 1000 + 60,
    positionSide: PositionSide.Long,
    side: OrderSide.Buy,
    qty: 1,
    price: 100,
    realizedPnl: 0,
  },
  {
    id: '102',
    symbol,
    timestamp: FROM / 1000 + 120,
    positionSide: PositionSide.Short,
    side: OrderSide.Sell,
    qty: 1,
    price: 110,
    realizedPnl: 0,
  },
  {
    id: '103',
    symbol,
    timestamp: FROM / 1000 + 300,
    positionSide: PositionSide.Long,
    side: OrderSide.Sell,
    qty: 1,
    price: 120,
    realizedPnl: 20,
  },
  {
    id: '104',
    symbol,
    timestamp: FROM / 1000 + 360,
    positionSide: PositionSide.Short,
    side: OrderSide.Buy,
    qty: 1,
    price: 100,
    realizedPnl: 10,
  },
];
const funding = (seconds: number, income: number) => ({
  symbol,
  category: 'funding',
  timestamp: FROM + seconds * 1000,
  income,
});

const build = (rows: Execution[], income: ReturnType<typeof funding>[], symbolScoped = true) =>
  buildFoldedPositionRecords(
    {
      fetchHistoricalFills: async ({ symbol: requested }: { symbol?: string }) =>
        requested ? rows.filter((fill) => fill.symbol === requested) : rows,
      fetchIncomeHistoryRange: async () => income,
    },
    { account: 'main', from: FROM, to: FROM + 600_000, symbolScoped, resolveSettleCurrency: () => 'USDT' },
  );

describe('symbol funding cannot be duplicated across hedge legs (TEAL-7 B3)', () => {
  it.each([true, false])(
    'retains one +5 ledger event separately instead of assigning +5 to each leg (symbolScoped=%s)',
    async (symbolScoped) => {
      const event = funding(180, 5);
      const result = await build(fills, [event], symbolScoped);
      expect(result.records.map((record) => record.positionSide)).toEqual([PositionSide.Long, PositionSide.Short]);
      expect(result.records.map((record) => record.funding)).toEqual([0, 0]);
      expect(result.records.map((record) => record.fundingAttributed)).toEqual([false, false]);
      expect(result.unattributedFunding).toEqual([{ symbol, timestamp: event.timestamp, income: 5 }]);
      expect(result.records.map((record) => record.realizedPnl)).toEqual([20, 10]);
    },
  );

  it('assigns only uniquely owned events and retains the ambiguous remainder once', async () => {
    const result = await build(fills, [funding(90, 2), funding(180, 5), funding(330, -1)]);
    expect(result.records.map((record) => record.funding)).toEqual([2, -1]);
    expect(result.records.map((record) => record.fundingAttributed)).toEqual([false, false]);
    expect(result.unattributedFunding).toEqual([{ symbol, timestamp: FROM + 180_000, income: 5 }]);
    const assigned = result.records.reduce((total, record) => total + record.funding, 0);
    const unassigned = result.unattributedFunding.reduce((total, row) => total + row.income, 0);
    expect(assigned + unassigned).toBe(6);
  });

  it('accounts for an opposing leg that is still open even though it emits no closed record', async () => {
    const result = await build(fills.slice(0, 3), [funding(180, 5)]);
    expect(result.records).toHaveLength(1);
    expect(result.records[0]).toMatchObject({ positionSide: PositionSide.Long, funding: 0, fundingAttributed: false });
    expect(result.unattributedFunding).toHaveLength(1);
    expect(result.unattributedFunding[0].income).toBe(5);
  });

  it('keeps funding attributable when only one hedge leg is open at the event', async () => {
    const result = await build(fills, [funding(90, 2), funding(330, -1)]);
    expect(result.records.map((record) => record.funding)).toEqual([2, -1]);
    expect(result.records.every((record) => record.fundingAttributed)).toBe(true);
    expect(result.unattributedFunding).toEqual([]);
  });

  it('does not assign an equal-time handover event to both lifecycles', async () => {
    const result = await build(
      [fills[0], { ...fills[2], id: '102', timestamp: FROM / 1000 + 120 }, { ...fills[1], id: '103' }, fills[3]],
      [funding(120, 5)],
    );
    expect(result.records.map((record) => record.funding)).toEqual([0, 0]);
    expect(result.records.every((record) => !record.fundingAttributed)).toBe(true);
    expect(result.unattributedFunding).toEqual([{ symbol, timestamp: FROM + 120_000, income: 5 }]);
  });

  it('retains events outside the visible intervals and keeps other symbols independent', async () => {
    const ethFills = fills
      .filter((fill) => fill.positionSide === PositionSide.Long)
      .map((fill) => ({ ...fill, symbol: 'ETHUSDT' }));
    const result = await build(
      [...fills, ...ethFills],
      [funding(180, 5), funding(30, -3), { ...funding(180, 2), symbol: 'ETHUSDT' }],
    );
    const eth = result.records.find((record) => record.symbol === 'ETHUSDT');
    expect(eth).toMatchObject({ funding: 2, fundingAttributed: true });
    expect(result.records.filter((record) => record.symbol === symbol).map((record) => record.funding)).toEqual([0, 0]);
    expect(result.unattributedFunding).toEqual([
      { symbol, timestamp: FROM + 180_000, income: 5 },
      { symbol, timestamp: FROM + 30_000, income: -3 },
    ]);
  });
});
