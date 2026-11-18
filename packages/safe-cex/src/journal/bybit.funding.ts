import type { IncomeRecord } from '../types';
import type { BybitLifecycle } from './bybit.lifecycles';
import type { BybitLifecycleContext } from './bybit.position-adapter';

import BigNumber from 'bignumber.js';

import { parseBybitDecimal } from './bybit.lifecycles';

/** Index once: native flow/fee projections can repeat one execution's evidence. */
export const bybitFinancialContext = (
  income: readonly IncomeRecord[],
  resolveSettleCurrency: (symbol: string) => string,
) => {
  const cashFlow = new Map<string, BigNumber>();
  const conflicts = new Set<string>();
  const fundingBySymbol = new Map<string, IncomeRecord[]>();
  const key = (id: string, symbol: string, asset: string) => JSON.stringify([id, symbol, asset]);
  for (const row of income) {
    if (row.category === 'funding') {
      const list = fundingBySymbol.get(row.symbol) ?? [];
      list.push(row);
      fundingBySymbol.set(row.symbol, list);
    }
    if (!row.tradeId || !('tradeCashFlow' in row)) continue;
    const id = key(row.tradeId, row.symbol, row.asset);
    if (conflicts.has(id)) continue;
    const amount = parseBybitDecimal(row.tradeCashFlow);
    const previous = cashFlow.get(id);
    if (!amount.isFinite() || (previous !== undefined && !previous.eq(amount))) {
      cashFlow.delete(id);
      conflicts.add(id);
    } else cashFlow.set(id, amount);
  }

  return (lifecycle: BybitLifecycle, rows: readonly Record<string, unknown>[]): BybitLifecycleContext | undefined => {
    const interval = lifecycle.interval;
    if (
      !interval.complete ||
      interval.closedAt === undefined ||
      lifecycle.closingExecutionIds.length === 0 ||
      rows.length === 0
    )
      return undefined;
    const asset = resolveSettleCurrency(interval.symbol);
    let gross = new BigNumber(0);
    for (const id of lifecycle.closingExecutionIds) {
      const value = cashFlow.get(key(id, interval.symbol, asset));
      // Missing is unknown. An explicitly reported zero is kept in the map.
      if (value === undefined) return undefined;
      gross = gross.plus(value);
    }
    let funding = new BigNumber(0);
    for (const row of fundingBySymbol.get(interval.symbol) ?? []) {
      if (!Number.isFinite(row.timestamp)) return undefined;
      if (row.timestamp < interval.openedAt || row.timestamp > interval.closedAt) continue;
      const amount = parseBybitDecimal(row.income);
      if (!amount.isFinite() || row.asset !== asset) return undefined;
      // The symbol ledger cannot prove a hedge-leg allocation. Do not invent one.
      if (interval.positionSide !== 'both') return undefined;
      funding = funding.plus(amount);
    }
    let net = new BigNumber(0);
    let fees = new BigNumber(0);
    for (const row of rows) {
      const pnl = parseBybitDecimal(row.closedPnl);
      const openFee = parseBybitDecimal(row.openFee);
      const closeFee = parseBybitDecimal(row.closeFee);
      if (!pnl.isFinite() || !openFee.isFinite() || !closeFee.isFinite()) return undefined;
      net = net.plus(pnl);
      fees = fees.plus(openFee).plus(closeFee);
    }
    // Independent native cash flow excludes fees and funding. Bound rounding
    // reconciliation at 1e-8 per input term; publish the native gross value.
    const tolerance = new BigNumber('0.00000001').times(rows.length + lifecycle.closingExecutionIds.length + 1);
    if (net.plus(fees).minus(funding).minus(gross).abs().gt(tolerance)) return undefined;
    const result: BybitLifecycleContext = {
      openedAt: interval.openedAt,
      realizedPnl: gross.toNumber(),
      fees: fees.toNumber(),
      funding: funding.toNumber(),
      fundingAttributed: true,
    };
    if (![result.realizedPnl, result.fees, result.funding].every(Number.isFinite)) return undefined;
    return result;
  };
};
