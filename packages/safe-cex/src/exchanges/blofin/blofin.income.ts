import type { ClosedPnlEntry, IncomeRecord } from '../../types';

import { PositionSide } from '../../types';

/**
 * BloFin ledger and closed positions — two genuinely different endpoints.
 *
 * Ledger comes from `/api/v1/trade/fills-history`; closed positions from
 * `/api/v1/account/positions-history`. Because those are distinct sources, the second can
 * cross-check the first (see `utils/resolveClosedPositions.ts`). `/api/v1/account/bills`
 * answers `152404 This operation is not supported`, so there is no bills-style ledger here.
 *
 * Field shapes and sign conventions verified live (sheet row 41):
 *
 *   fills:     `instId, tradeId, orderId, fillPrice, fillSize, fillPnl, side, positionSide,
 *               fee, ts, brokerId`  — `fillPnl` is GROSS, `fee` is POSITIVE when charged.
 *   positions: `historyId, positionId, instId, ..., openAveragePrice, closeAveragePrice,
 *               createTime, updateTime, leverage, realizedPnl, realizedPnlRatio, fee`
 *               — `realizedPnl` is NET of `fee`, and `fee` is NEGATIVE when charged.
 *
 * The two endpoints use OPPOSITE fee signs. That is not a typo to normalise away at the
 * call site; each mapper handles its own.
 *
 * **Known gap:** neither endpoint carries funding, so the `funding` category is always
 * empty on BloFin and the full equity identity cannot close here.
 */

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapBlofinFillToIncomeRecords = (
  row: Record<string, any>,
  resolveSymbol: (instId: string) => string | undefined,
): IncomeRecord[] => {
  const timestamp = parseInt(String(row.ts ?? '0'), 10);
  const instId = String(row.instId ?? '');
  if (!timestamp || !instId) {
    return [];
  }

  const symbol = resolveSymbol(instId) || instId;
  const id = String(row.tradeId ?? `${timestamp}:${instId}`);
  const fillPnl = toNumber(row.fillPnl);
  const fee = toNumber(row.fee);

  const records: IncomeRecord[] = [];

  if (fillPnl !== 0) {
    records.push({
      symbol,
      incomeType: 'REALIZED_PNL',
      income: fillPnl,
      asset: 'USDT',
      timestamp,
      tranId: `${id}:pnl`,
      tradeId: id,
      category: 'realized_pnl',
    });
  }

  if (fee !== 0) {
    // Positive fee = charged on this endpoint, so the emitted income is negative.
    records.push({
      symbol,
      incomeType: fee > 0 ? 'COMMISSION' : 'REBATE',
      income: -fee,
      asset: 'USDT',
      timestamp,
      tranId: `${id}:fee`,
      tradeId: id,
      category: fee > 0 ? 'commission' : 'rebate',
    });
  }

  return records;
};

export const mapBlofinClosedPosition = (
  row: Record<string, any>,
  resolveSymbol: (instId: string) => string | undefined,
  accountName: string,
): ClosedPnlEntry | null => {
  const openTime = parseInt(String(row.createTime ?? '0'), 10);
  const instId = String(row.instId ?? '');
  if (!openTime || !instId) {
    return null;
  }

  const closePrice = toNumber(row.closeAveragePrice);
  const contracts = toNumber(row.closePositions);

  return {
    provenance: 'venue',
    symbol: resolveSymbol(instId) || instId,
    notional: contracts * closePrice,
    contracts,
    entryPrice: toNumber(row.openAveragePrice),
    exitPrice: closePrice,
    // Legacy field name; carries realized PnL. NET of `fee` on this endpoint.
    unrealizedPnl: toNumber(row.realizedPnl),
    side: String(row.positionSide ?? '').toLowerCase() === 'short' ? PositionSide.Short : PositionSide.Long,
    // Charged fees arrive NEGATIVE here (opposite of the fills endpoint).
    fees: Math.abs(toNumber(row.fee)),
    time: openTime,
    closedTime: parseInt(String(row.updateTime ?? '0'), 10) || openTime,
    account: accountName,
    leverage: toNumber(row.leverage),
  };
};
