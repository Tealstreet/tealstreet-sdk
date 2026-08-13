import type { IncomeCategory, IncomeRecord } from '../../types';
import type { BybitTransactionRecord } from './bybit.types';

/**
 * Bybit transaction log -> IncomeRecord decomposition.
 *
 * The previous inline mapper read ONLY `cashFlow`, so every trading fee was dropped:
 * `record.fee` was never referenced. That makes the reconciliation identity
 * (delta equity == realized + funding - fees +/- ...) unclosable on bybit, because the
 * commission term is simply absent rather than wrong.
 *
 * Bybit reports three independent terms on one row, so one row becomes up to three
 * records. `change` is bybit's own net for the row; it is NOT used to derive the fee,
 * because its exact composition is undocumented in this repo. The decomposition uses the
 * documented terms directly and the emitted incomes sum to `cashFlow + funding - fee`.
 *
 * Sign convention: `fee` is positive when charged, so the emitted income is `-fee`.
 */

const TYPE_CATEGORY: Record<string, IncomeCategory> = {
  TRADE: 'realized_pnl',
  DELIVERY: 'realized_pnl',
  SETTLEMENT: 'funding',
  LIQUIDATION: 'liquidation',
  TRANSFER_IN: 'transfer',
  TRANSFER_OUT: 'transfer',
  FEE_REFUND: 'rebate',
};

const CATEGORY_INCOME_TYPE: Record<IncomeCategory, string> = {
  realized_pnl: 'REALIZED_PNL',
  funding: 'FUNDING_FEE',
  commission: 'COMMISSION',
  rebate: 'REBATE',
  transfer: 'TRANSFER',
  liquidation: 'LIQUIDATION',
  other: 'OTHER',
};

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const bybitTransactionCategory = (type: unknown): IncomeCategory => {
  return TYPE_CATEGORY[String(type ?? '')] || 'other';
};

export const mapBybitTransactionToIncomeRecords = (record: BybitTransactionRecord): IncomeRecord[] => {
  const timestamp = parseInt(String(record.transactionTime ?? '0'), 10);
  if (!Number.isFinite(timestamp) || timestamp <= 0) {
    return [];
  }

  const cashFlow = toNumber(record.cashFlow);
  const funding = toNumber(record.funding);
  const fee = toNumber(record.fee);

  const category = bybitTransactionCategory(record.type);
  const symbol = record.symbol || '';
  const asset = record.currency || 'USDT';
  const tradeId = record.tradeId || undefined;
  // The id must be unique per record, because `fetchIncomeHistoryRange` dedupes on it.
  // `tradeId || orderId || transactionTime` is not sufficient on its own: funding settles
  // for every symbol at the same instant and those rows carry neither a tradeId nor an
  // orderId, so they would share one id and all but one would be dropped. Symbol and type
  // disambiguate rows sharing a timestamp; the suffix disambiguates terms within a row.
  const baseId = [record.tradeId || record.orderId || '', symbol, record.type, record.transactionTime]
    .filter(Boolean)
    .join('-');

  const records: IncomeRecord[] = [];

  const push = (income: number, recordCategory: IncomeCategory, suffix: string) => {
    records.push({
      symbol,
      incomeType: CATEGORY_INCOME_TYPE[recordCategory],
      income,
      asset,
      timestamp,
      tranId: `${baseId}:${suffix}`,
      ...(tradeId ? { tradeId } : {}),
      category: recordCategory,
    });
  };

  if (cashFlow !== 0) {
    push(cashFlow, category, 'flow');
  }

  if (funding !== 0) {
    push(funding, 'funding', 'funding');
  }

  if (fee !== 0) {
    push(-fee, fee > 0 ? 'commission' : 'rebate', 'fee');
  }

  return records;
};
