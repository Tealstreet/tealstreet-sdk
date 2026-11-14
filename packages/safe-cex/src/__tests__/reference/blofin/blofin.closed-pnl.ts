import type { ClosedPnlEntry } from '../../../types';
import { PositionSide } from '../../../types';

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapBlofinClosedPosition = (
  row: Record<string, unknown>,
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
