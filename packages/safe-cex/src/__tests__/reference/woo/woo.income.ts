import type { IncomeCategory, IncomeHistoryOptions, IncomeRecord } from '../../../types';

import { normalizeSymbol } from './woo.utils';

// V1 token history is a cash ledger. Its symbol-less daily PnL is account-wide.
export const wooIncomeCategory = (kind: string): IncomeCategory => {
  switch (kind) {
    case 'REALIZED_PNL':
      return 'realized_pnl';
    case 'FUNDING_FEE':
      return 'funding';
    case 'TRANSACTION_FEE':
    case 'TRADING_FEE':
      return 'commission';
    case 'REBATE':
    case 'REFERRAL':
      return 'rebate';
    case 'WITHDRAW':
    case 'DEPOSIT':
    case 'FIAT_WITHDRAW':
    case 'FIAT_DEPOSIT':
    case 'SUB_ACCOUNT_TRANSFER':
    case 'CONVERT':
      return 'transfer';
    case 'LIQUIDATION':
      return 'liquidation';
    default:
      return 'other';
  }
};

export const buildWooIncomeParams = (opts: IncomeHistoryOptions = {}, page = 1): Record<string, string | number> => {
  const params: Record<string, string | number> = { size: Math.min(opts.limit ?? 100, 100), page };
  if (opts.incomeType !== undefined) params.type = opts.incomeType;
  if (opts.startTime !== undefined) params.start_t = opts.startTime;
  if (opts.endTime !== undefined) params.end_t = opts.endTime;
  return params;
};

const finite = (value: unknown): number | undefined => {
  if (typeof value !== 'number' && typeof value !== 'string') return undefined;
  if (typeof value === 'string' && !value.trim()) return undefined;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : undefined;
};

export const mapWooIncomeRecord = (input: unknown): IncomeRecord | null => {
  if (!input || typeof input !== 'object' || Array.isArray(input)) return null;
  const row = input as Record<string, unknown>;
  const income = finite(row.amount);
  const timestamp = finite(row.timestamp);
  const id = row.id;
  if (
    income === undefined ||
    timestamp === undefined ||
    timestamp <= 0 ||
    typeof row.type !== 'string' ||
    !row.type ||
    typeof row.token !== 'string' ||
    !row.token ||
    !(
      (typeof id === 'string' && id.length > 0) ||
      typeof id === 'bigint' ||
      (typeof id === 'number' && Number.isSafeInteger(id) && id >= 0)
    ) ||
    (row.symbol !== undefined && row.symbol !== null && typeof row.symbol !== 'string')
  )
    return null;
  return {
    symbol: typeof row.symbol === 'string' && row.symbol ? normalizeSymbol(row.symbol) : '',
    incomeType: row.type,
    income,
    asset: row.token,
    timestamp,
    tranId: String(id),
    category: wooIncomeCategory(row.type),
  };
};

export const wooIncomeIdentity = (record: IncomeRecord): string => `${record.tranId}:${record.incomeType}`;
