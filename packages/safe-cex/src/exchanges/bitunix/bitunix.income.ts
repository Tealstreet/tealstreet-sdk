import type { ClosedPnlEntry, IncomeRecord } from '../../types';

import { PositionSide } from '../../types';

/**
 * BitUnix ledger and closed positions — two different endpoints, so the second can
 * cross-check the first (see `utils/resolveClosedPositions.ts`).
 *
 *   ledger:    `/api/v1/futures/trade/get_history_trades`
 *   positions: `/api/v1/futures/position/get_history_positions`
 *
 * `/api/v1/futures/account/get_bills` is 404 on this account, so there is no bills-style
 * ledger here.
 *
 * Conventions verified live (sheet row 56). A position with `entryPrice == closePrice`
 * (gross zero) reported `fee 0.00201641` and `realizedPNL -0.00201641`, which pins
 * `realizedPNL` as NET of fee, with `fee` POSITIVE when charged on both endpoints.
 *
 * **Known gap:** the trades endpoint carries no funding, so the ledger's `funding`
 * category is always empty on BitUnix. The positions endpoint has a `funding` field, but
 * whether `realizedPNL` folds it in is UNVERIFIED — it was 0 on every row sampled. Do not
 * assume either way without a funded window.
 */

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapBitunixTradeToIncomeRecords = (row: Record<string, any>): IncomeRecord[] => {
  const timestamp = parseInt(String(row.ctime ?? '0'), 10);
  const symbol = String(row.symbol ?? '');
  if (!timestamp || !symbol) {
    return [];
  }

  const id = String(row.tradeId ?? `${timestamp}:${symbol}`);
  const realized = toNumber(row.realizedPNL);
  const fee = toNumber(row.fee);
  const asset = String(row.marginCoin || 'USDT');

  const records: IncomeRecord[] = [];

  if (realized !== 0) {
    records.push({
      symbol,
      incomeType: 'REALIZED_PNL',
      income: realized,
      asset,
      timestamp,
      tranId: `${id}:pnl`,
      tradeId: id,
      category: 'realized_pnl',
    });
  }

  if (fee !== 0) {
    records.push({
      symbol,
      incomeType: fee > 0 ? 'COMMISSION' : 'REBATE',
      income: -fee,
      asset,
      timestamp,
      tranId: `${id}:fee`,
      tradeId: id,
      category: fee > 0 ? 'commission' : 'rebate',
    });
  }

  return records;
};

export const mapBitunixClosedPosition = (row: Record<string, any>, accountName: string): ClosedPnlEntry | null => {
  const openTime = parseInt(String(row.ctime ?? '0'), 10);
  const symbol = String(row.symbol ?? '');
  if (!openTime || !symbol) {
    return null;
  }

  const closePrice = toNumber(row.closePrice);
  const contracts = toNumber(row.qty);

  return {
    provenance: 'venue',
    symbol,
    notional: contracts * closePrice,
    contracts,
    entryPrice: toNumber(row.entryPrice),
    exitPrice: closePrice,
    // Legacy field name; carries realized PnL. NET of `fee`.
    unrealizedPnl: toNumber(row.realizedPNL),
    // `side` here is the position's own side, not a closing order's — no inversion.
    side: String(row.side ?? '').toUpperCase() === 'SELL' ? PositionSide.Short : PositionSide.Long,
    fees: toNumber(row.fee),
    time: openTime,
    closedTime: parseInt(String(row.mtime ?? '0'), 10) || openTime,
    account: accountName,
    leverage: toNumber(row.leverage),
  };
};
