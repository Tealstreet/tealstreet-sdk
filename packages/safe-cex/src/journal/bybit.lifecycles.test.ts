import type { Execution } from '../types';
import type { BybitExecutionEvidence } from './bybit.lifecycles';

import { describe, expect, it } from 'vitest';

import { historyCompletenessOf } from '../historyCompleteness';
import { OrderSide } from '../types';
import { reconstructBybitLifecycles } from './bybit.lifecycles';
import { aggregateBybitClosedLifecycles } from './bybit.position-adapter';

const fill = (id: string, orderId: string, timestamp: number, side: OrderSide): Execution => ({
  id,
  orderId,
  timestamp,
  symbol: 'BTCUSDT',
  side,
  qty: 1,
  price: side === OrderSide.Buy ? 100 : 110,
});
const fills = [
  fill('open-b', 'open-1', 10, OrderSide.Buy),
  fill('open-a', 'open-2', 10, OrderSide.Buy),
  fill('close-b', 'close-1', 20, OrderSide.Sell),
  fill('close-a', 'close-2', 20, OrderSide.Sell),
];
const rows = ['close-1', 'close-2'].map((orderId) => ({
  orderId,
  symbol: 'BTCUSDT',
  side: 'Sell',
  closedSize: '1',
  avgEntryPrice: '100',
  avgExitPrice: '110',
  closedPnl: '10',
  updatedTime: '20000',
}));
const resolveLeg = (): 'both' => 'both';
const evidence = (positionAfter: string): BybitExecutionEvidence => ({
  positionAfter,
  quantity: '1',
  symbol: 'BTCUSDT',
});
const positions = new Map<string, BybitExecutionEvidence>([
  ['open-b', evidence('1')],
  ['open-a', evidence('2')],
  ['close-b', evidence('1')],
  ['close-a', evidence('0')],
]);
const fold = (source: Execution[], closed = rows, anchors: ReadonlyMap<string, BybitExecutionEvidence> = positions) =>
  aggregateBybitClosedLifecycles(closed, {
    account: 'account',
    resolveSettleCurrency: () => 'USDT',
    resolveLeg,
    lifecycles: reconstructBybitLifecycles(source, resolveLeg, anchors),
  });

describe('Bybit fills-proven lifecycle membership', () => {
  it('keeps identity stable when same-direction fills arrive reversed at equal timestamps', () => {
    const forward = fold(fills);
    const reversed = fold([...fills].reverse());
    expect(forward).toHaveLength(1);
    expect(reversed[0].key).toBe(forward[0].key);
    expect(forward[0]).toMatchObject({ closedSize: 2, realizedPnl: 20, closeCount: 2 });
  });

  it('deduplicates repeated native executions and close events before summing money', () => {
    expect(fold([...fills, ...fills], [...rows, ...rows])).toEqual(fold(fills));
  });

  it('refuses opposing fills without a chronological ordering at the same native timestamp', () => {
    const ambiguous = fills.map((item) => ({ ...item, timestamp: 10 }));
    const records = fold(ambiguous);
    expect(records).toEqual([]);
    expect(historyCompletenessOf(records)).toEqual({ incomplete: true, reason: 'unresolvedLifecycle' });
  });

  it('does not publish a partial native close census as a complete lifecycle', () => {
    const records = fold(fills, rows.slice(0, 1));
    expect(records).toEqual([]);
    expect(historyCompletenessOf(records)?.incomplete).toBe(true);
  });

  it('keeps a visible partial close in an open lifecycle out of the closed-position count', () => {
    const records = fold(fills.slice(0, 3), rows.slice(0, 1));
    expect(records).toEqual([]);
    expect(historyCompletenessOf(records)).toBeUndefined();
  });

  it('does not invent a lifecycle when the opening fills are absent', () => {
    const records = fold(fills.slice(2));
    expect(records).toEqual([]);
    expect(historyCompletenessOf(records)?.incomplete).toBe(true);
  });

  it('does not treat a missing order-leg join as proven one-way mode', () => {
    expect(reconstructBybitLifecycles(fills, () => undefined, positions)).toEqual([]);
  });

  it('does not assume a zero position when no native post-trade anchor exists', () => {
    const records = fold(fills, rows, new Map());
    expect(records).toEqual([]);
    expect(historyCompletenessOf(records)?.incomplete).toBe(true);
  });

  it('does not count an add and a partial close of a pre-window position as a round trip', () => {
    const source = [fill('add', 'add', 10, OrderSide.Buy), fill('partial', 'partial', 20, OrderSide.Sell)];
    const records = fold(
      source,
      [{ ...rows[0], orderId: 'partial' }],
      new Map([
        ['add', evidence('2')],
        ['partial', evidence('1')],
      ]),
    );
    expect(records).toEqual([]);
    expect(historyCompletenessOf(records)?.incomplete).toBe(true);
  });

  it('refuses inconsistent position anchors rather than inventing a flat boundary', () => {
    const records = fold(
      fills,
      rows,
      new Map<string, BybitExecutionEvidence>([...positions, ['close-a', evidence('5')]]),
    );
    expect(records).toEqual([]);
    expect(historyCompletenessOf(records)?.incomplete).toBe(true);
  });
});
