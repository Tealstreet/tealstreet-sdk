import type { Axios } from 'axios';
import type { IncomeCategory, IncomeHistoryOptions, IncomeRecord } from '../../../types';
import type { BybitTransactionHistoryOptions, BybitTransactionRecord } from '../../../exchanges/bybit/bybit.types';

import { sleep } from '@tealstreet/sleep/src/sleep';
import { LogSeverity } from '../../../types';
import { omitUndefined } from '../../../utils/omitUndefined';
import { splitTimeRange } from '../../../utils/splitTimeRange';
import { ENDPOINTS, ORDER_HISTORY_MAX_WINDOW_MS } from '../../../exchanges/bybit/bybit.types';

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
  // The id must be unique per record. Nothing dedupes on it — bybit tranIds are not
  // unique, so a Map keyed on them would delete real rows — but a stable unique id is
  // what downstream storage keys on.
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

/**
 * What the income calls need from an exchange, and nothing more. The linear and
 * inverse classes do not share a base beyond BaseExchange, so this is passed
 * rather than inherited. `log` is an arrow property on BaseExchange, so handing
 * over the reference keeps its binding.
 */
export interface BybitIncomeContext {
  xhr: Axios;
  log: (message: string, severity?: LogSeverity) => void;
  category: 'linear' | 'inverse';
  accountType: 'UNIFIED' | 'CONTRACT';
}

export const fetchBybitIncomeHistory = async (
  ctx: BybitIncomeContext,
  opts?: IncomeHistoryOptions,
): Promise<IncomeRecord[]> => {
  try {
    const params: BybitTransactionHistoryOptions = {
      // Transaction log endpoint only supports UNIFIED account type
      accountType: ctx.accountType,
      category: ctx.category,
      // V5 transaction-log caps `limit` at 50. Sending more is rejected outright, and
      // the non-zero retCode path below breaks out with an EMPTY list — so an oversized
      // limit reads downstream as "this account has no ledger", not as an error.
      limit: Math.min(opts?.limit || 50, 50),
    };

    if (opts?.incomeType) {
      // Map generic income types to Bybit transaction types
      const typeMap: Record<string, string> = {
        REALIZED_PNL: 'TRADE',
        FUNDING_FEE: 'SETTLEMENT',
        COMMISSION: 'TRADE',
      };
      params.type = (typeMap[opts.incomeType] || opts.incomeType) as any;
    }

    const allRecords: IncomeRecord[] = [];
    const maxPages = 10; // Safety limit
    const recordCap = opts?.limit || 1000;

    // Bybit rejects a >7-day range on the transaction log too, and the `break` below
    // turns that into an empty list — "this account has no ledger" rather than an error.
    // `fetchIncomeHistoryRange` already chunked before calling; this makes the method
    // safe for DIRECT callers, who previously had no protection at all. Chunks already
    // inside the cap come back as a single window, so that path is unchanged.
    const windows =
      opts?.startTime !== undefined && opts?.endTime !== undefined
        ? splitTimeRange(opts.startTime, opts.endTime, ORDER_HISTORY_MAX_WINDOW_MS)
        : [{ startTime: opts?.startTime, endTime: opts?.endTime }];

    for (const window of windows) {
      let cursor: string | undefined;
      let page = 0;

      do {
        const requestParams = omitUndefined({
          ...params,
          startTime: window.startTime,
          endTime: window.endTime,
          cursor,
        });

        const { data } = await ctx.xhr.get(ENDPOINTS.TRANSACTION_LOG, { params: requestParams });

        if (data.retMsg !== 'OK' && +data.retCode !== 0) {
          ctx.log(`Failed to fetch transaction log: ${data.retMsg}`, LogSeverity.Error);
          break;
        }

        const records = data.result?.list as BybitTransactionRecord[] | undefined;
        if (!records || records.length === 0) break;

        for (const record of records) {
          // One transaction row carries cashFlow, funding AND fee independently, so it
          // maps to several records. Reading only cashFlow silently dropped every fee.
          allRecords.push(...mapBybitTransactionToIncomeRecords(record));
        }

        cursor = data.result?.nextPageCursor;
        page++;
      } while (cursor && page < maxPages && allRecords.length < recordCap);

      if (allRecords.length >= recordCap) break;
    }

    return allRecords;
  } catch (error: any) {
    ctx.log(`Error fetching income history: ${error.message}`, LogSeverity.Error);
    return [];
  }
};

/**
 * Fetch income history for a date range.
 * Bybit limits queries to 7 days per request, so we chunk the range.
 *
 * @param startTime - Start timestamp in ms
 * @param endTime - End timestamp in ms
 * @param incomeType - Optional filter by income type
 * @returns Array of all income records in the range
 */
export const fetchBybitIncomeHistoryRange = async (
  ctx: BybitIncomeContext,
  startTime: number,
  endTime: number,
  incomeType?: string,
): Promise<IncomeRecord[]> => {
  const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000; // 7 days per chunk (Bybit limit)
  // Chunk bounds are inclusive at both ends, so consecutive chunks must not share an
  // endpoint or the boundary row is returned twice. Advancing past it is the fix —
  // deduping on tranId is NOT, because bybit tranIds are not unique (hedge-mode
  // settlement rows share every identity component, and transfers have no id at all),
  // so a Map keyed on them silently deletes real value.
  const allRecords: IncomeRecord[] = [];

  let currentStart = startTime;

  while (currentStart < endTime) {
    const currentEnd = Math.min(currentStart + SEVEN_DAYS_MS, endTime);

    const records = await fetchBybitIncomeHistory(ctx, {
      startTime: currentStart,
      endTime: currentEnd,
      incomeType,
      limit: 1000,
    });

    allRecords.push(...records);

    currentStart = currentEnd + 1;

    // Add a small delay to respect rate limits
    await sleep(100);
  }

  return allRecords.sort((a, b) => b.timestamp - a.timestamp);
};
