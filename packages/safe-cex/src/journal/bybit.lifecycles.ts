import type { Execution, IncomeRecord, PositionSide } from '../types';
import type { PositionInterval } from './position-intervals';

import BigNumber from 'bignumber.js';

import { reconstructPositionIntervals } from './position-intervals';

export type BybitExecutionEvidence = {
  readonly positionAfter: string;
  readonly quantity: string;
  readonly symbol: string;
};

const decimal = (value: unknown): BigNumber =>
  (typeof value === 'number' && Number.isFinite(value)) ||
  (typeof value === 'string' && /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(value))
    ? new BigNumber(value)
    : new BigNumber(NaN);

export const parseBybitDecimal = decimal;

/** The common income model stays unchanged; these are optional native Bybit facts. */
export const bybitExecutionEvidence = (
  income: readonly IncomeRecord[],
): ReadonlyMap<string, BybitExecutionEvidence> => {
  const result = new Map<string, BybitExecutionEvidence>();
  const conflicts = new Set<string>();
  for (const item of income) {
    if (
      !item.tradeId ||
      conflicts.has(item.tradeId) ||
      !('positionAfter' in item) ||
      typeof item.positionAfter !== 'string' ||
      !('executionQuantity' in item) ||
      typeof item.executionQuantity !== 'string'
    )
      continue;
    if (
      !decimal(item.positionAfter).isFinite() ||
      !decimal(item.executionQuantity).isFinite() ||
      !decimal(item.executionQuantity).isGreaterThan(0)
    )
      continue;
    const evidence = {
      positionAfter: item.positionAfter,
      quantity: item.executionQuantity,
      symbol: item.symbol,
    };
    const previous = result.get(item.tradeId);
    if (
      previous &&
      (!decimal(previous.positionAfter).eq(evidence.positionAfter) ||
        !decimal(previous.quantity).eq(evidence.quantity) ||
        previous.symbol !== evidence.symbol)
    ) {
      result.delete(item.tradeId);
      conflicts.add(item.tradeId);
    } else result.set(item.tradeId, evidence);
  }
  return result;
};

export type BybitLifecycle = {
  readonly interval: PositionInterval;
  readonly openingId: string;
  readonly closingId?: string;
  readonly closingOrderIds: readonly string[];
  readonly closingOrderSizes: ReadonlyMap<string, string>;
  readonly closingExecutionIds: readonly string[];
};

/** Source-order membership, never entry-price buckets or a guessed time gap. */
export const reconstructBybitLifecycles = (
  fills: Execution[],
  resolveLeg: (orderId: string) => PositionSide | 'both' | undefined,
  positionAfter: ReadonlyMap<string, BybitExecutionEvidence>,
): BybitLifecycle[] => {
  const lifecycles: BybitLifecycle[] = [];
  const byId = new Map<string, Execution>();
  for (const fill of fills) {
    if (
      !fill.id ||
      !fill.orderId ||
      !Number.isFinite(fill.timestamp) ||
      !Number.isFinite(fill.qty) ||
      !Number.isFinite(fill.price) ||
      !(fill.qty > 0) ||
      !(fill.price > 0) ||
      (fill.side !== 'buy' && fill.side !== 'sell')
    )
      return [];
    const prior = byId.get(fill.id);
    if (prior && JSON.stringify(prior) !== JSON.stringify(fill)) return [];
    byId.set(fill.id, fill);
  }
  const unique = [...byId.values()];
  const legKey = (fill: Execution) => `${fill.symbol}|${resolveLeg(fill.orderId ?? '')}`;
  const unresolvedSymbols = new Set(
    unique.filter((fill) => resolveLeg(fill.orderId ?? '') === undefined).map((fill) => fill.symbol),
  );
  const byLeg = new Map<string, Execution[]>();
  for (const fill of unique) {
    const key = legKey(fill);
    const list = byLeg.get(key) ?? [];
    list.push(fill);
    byLeg.set(key, list);
  }
  const openingSize: Record<string, number> = {};
  const trusted = new Set<string>();
  const ordered: Execution[] = [];
  for (const [key, list] of byLeg) {
    const byTime = new Map<number, Execution[]>();
    for (const fill of [...list].sort((a, b) => a.timestamp - b.timestamp)) {
      const group = byTime.get(fill.timestamp) ?? [];
      group.push(fill);
      byTime.set(fill.timestamp, group);
    }
    let cumulative = new BigNumber(0);
    let initial: BigNumber | undefined;
    let consistent = list.every((fill) => {
      const evidence = positionAfter.get(fill.id ?? '');
      return (
        evidence !== undefined &&
        evidence.symbol === fill.symbol &&
        decimal(evidence.quantity).eq(fill.qty) &&
        decimal(evidence.positionAfter).isFinite()
      );
    });
    if (!consistent) continue;
    for (const group of byTime.values()) {
      const delta = group.reduce((sum, fill) => sum.plus(fill.side === 'buy' ? fill.qty : -fill.qty), new BigNumber(0));
      if (
        new Set(group.map((fill) => fill.side)).size === 1 &&
        group.every((fill) => positionAfter.has(fill.id ?? ''))
      ) {
        const direction = group[0].side === 'buy' ? 1 : -1;
        group.sort(
          (a, b) =>
            direction *
            (decimal(positionAfter.get(a.id ?? '')?.positionAfter ?? NaN).comparedTo(
              positionAfter.get(b.id ?? '')?.positionAfter ?? NaN,
            ) ?? 0),
        );
        const after = decimal(positionAfter.get(group[group.length - 1].id ?? '')?.positionAfter ?? NaN);
        const before = after.minus(delta);
        const candidate = before.minus(cumulative);
        if (!candidate.isFinite() || (initial !== undefined && !candidate.eq(initial))) consistent = false;
        initial ??= candidate;
        let current = before;
        for (const fill of group) {
          current = current.plus(fill.side === 'buy' ? fill.qty : -fill.qty);
          const evidence = positionAfter.get(fill.id ?? '')?.positionAfter ?? NaN;
          if (!current.eq(evidence)) consistent = false;
        }
      }
      cumulative = cumulative.plus(delta);
      ordered.push(...group);
    }
    // This independent post-trade balance is what proves flat at the window
    // boundary. Starting the signed-size fold at zero without it invents closes
    // when an older position is merely added to and partially reduced.
    if (consistent && initial !== undefined && initial.isFinite() && Number.isFinite(initial.toNumber())) {
      openingSize[key] = initial.toNumber();
      trusted.add(key);
    }
  }
  // Opposing fills at the same native timestamp still lack a chronological
  // tie-break. Keep their intervals unresolved rather than order opaque UUIDs.
  const sides = new Map<string, Set<string>>();
  for (const fill of unique) {
    const key = JSON.stringify([fill.symbol, resolveLeg(fill.orderId ?? ''), fill.timestamp]);
    const seen = sides.get(key) ?? new Set<string>();
    seen.add(fill.side);
    sides.set(key, seen);
  }
  reconstructPositionIntervals(ordered, {
    openingSize,
    sourceOrdered: true,
    resolveLeg: (fill) => resolveLeg(fill.orderId ?? ''),
    onInterval: (interval, sources) => {
      const first = sources[0];
      if (
        !first?.id ||
        unresolvedSymbols.has(interval.symbol) ||
        !trusted.has(`${interval.symbol}|${interval.positionSide}`)
      )
        return;
      if (sources.some((fill) => !fill.id || !fill.orderId || resolveLeg(fill.orderId) !== interval.positionSide))
        return;
      if (
        sources.some(
          (fill) =>
            (sides.get(JSON.stringify([fill.symbol, resolveLeg(fill.orderId ?? ''), fill.timestamp]))?.size ?? 0) > 1,
        )
      )
        return;
      const last = sources[sources.length - 1];
      const after = decimal(positionAfter.get(last?.id ?? '')?.positionAfter ?? NaN);
      if (interval.closedAt !== undefined && !interval.closedByFlip && !after.isZero()) return;
      if (interval.closedByFlip && (interval.side === 'long' ? after.isPositive() : after.isNegative())) return;
      const firstAfter = decimal(positionAfter.get(first.id)?.positionAfter ?? NaN);
      const before = firstAfter.minus(first.side === 'buy' ? first.qty : -first.qty);
      const openedByFlip =
        interval.side === 'long'
          ? before.isNegative() && firstAfter.isPositive()
          : before.isPositive() && firstAfter.isNegative();
      if (interval.complete && !before.isZero() && !openedByFlip) return;
      if (
        sources.slice(0, -1).some((fill) => {
          const size = decimal(positionAfter.get(fill.id ?? '')?.positionAfter ?? NaN);
          return interval.side === 'long' ? !size.isGreaterThan(0) : !size.isLessThan(0);
        })
      )
        return;
      const closingSide = interval.side === 'long' ? 'sell' : 'buy';
      const closing = sources.filter((fill) => fill.side === closingSide);
      const closingSizes = new Map<string, BigNumber>();
      for (const fill of closing) {
        const post = decimal(positionAfter.get(fill.id ?? '')?.positionAfter ?? NaN);
        const prior = post.minus(fill.side === 'buy' ? fill.qty : -fill.qty);
        if (interval.side === 'long' ? !prior.isGreaterThan(0) : !prior.isLessThan(0)) return;
        const quantity = BigNumber.minimum(prior.abs(), fill.qty);
        const id = fill.orderId ?? '';
        closingSizes.set(id, (closingSizes.get(id) ?? new BigNumber(0)).plus(quantity));
      }
      const openingId = sources
        .filter((fill) => fill.side !== closingSide)
        .flatMap((fill) => (fill.id ? [fill.id] : []))
        .sort()[0];
      if (!openingId) return;
      const closingOrderIds = [...new Set(closing.map((fill) => fill.orderId ?? ''))].sort();
      lifecycles.push({
        interval,
        // Canonical representatives of the full source sets, not input order:
        // same-direction fills may share a native timestamp and arrive reversed.
        openingId,
        closingId:
          interval.closedAt === undefined ? undefined : closing.flatMap((fill) => (fill.id ? [fill.id] : [])).sort()[0],
        closingOrderIds,
        closingOrderSizes: new Map([...closingSizes].map(([id, size]) => [id, size.toFixed()])),
        closingExecutionIds: closing.flatMap((fill) => (fill.id ? [fill.id] : [])).sort(),
      });
    },
  });
  return lifecycles;
};
