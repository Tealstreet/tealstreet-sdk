import type { ClosedPnlEntry } from '../../../types';
import { PositionSide } from '../../../types';

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapBitunixClosedPosition = (row: Record<string, unknown>, accountName: string): ClosedPnlEntry | null => {
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
