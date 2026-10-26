import type { IncomeCategory, IncomeRecord } from '../../../types';

/**
 * Bitget `/api/v2/mix/account/bill` -> `IncomeRecord[]`.
 *
 * Bitget had NO income axis before this: `fetchIncomeHistory` fell through to the branded
 * base stub, so the venue was fold-only and nothing could check its records. This is the
 * whole reason it was picked as venue 8 — its credential works, it is funded, and its ledger
 * contains real funding, which is the term that has broken more venues here than any other.
 *
 * **A SEVENTH CONVENTION PROFILE.** Measured from 819 live rows over 90 days (sheet row 36),
 * not inherited from any of the six:
 *
 * - **`amount` is GROSS realized PnL — the fee is NOT deducted from it.** Pinned against the
 *   row's own running `balance`, which is the only oracle here that cannot be argued with:
 *   consecutive balance deltas matched `amount + fee` on 676 of 818 comparable rows and
 *   matched `amount` alone on **6**. (The shortfall is ordering, not disagreement — rows
 *   sharing a millisecond cannot be sequenced from `cTime`.)
 * - **`fee` is NEGATIVE when charged**, on 818 of 818 trade rows — blofin's sign, the
 *   opposite of bitunix's. This is the exact field the two of them disagreed on, and it is
 *   invisible to the ledger identity because fees do not enter a gross-vs-gross comparison.
 * - **Funding is its own row type**, `contract_settle_fee`, carried in `amount` with `fee`
 *   always 0 — unlike okx/bybit/bitunix, which fold funding into realized PnL and had to be
 *   caught doing it.
 * - **`open_*` rows carry `amount: 0`** and exist only to charge the fee; PnL appears on the
 *   closing row. `buy`/`sell` are the one-way-mode spelling of the same events, and the
 *   account sampled contains both.
 *
 * **An unrecognised `businessType` is emitted as `other`, never dropped.** Dropping it would
 * shrink the ledger side of the identity silently, and "a category that goes quiet" is the
 * failure mode this epic keeps finding. Mapped to `other` it still appears in the census with
 * its raw type, so a new bitget event class shows up as an unexplained residual with a name
 * attached rather than as a hole.
 */

const num = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

/**
 * Trading rows. All of these carry a symbol and a (negative) fee; the closing forms also
 * carry gross PnL in `amount`, the opening forms carry `amount: 0`.
 */
const TRADE_TYPES = new Set(['buy', 'sell', 'open_long', 'open_short', 'close_long', 'close_short']);

/** Everything else measured on the live account, plus the obvious transfer siblings. */
const CATEGORY_BY_TYPE: Record<string, IncomeCategory> = {
  contract_settle_fee: 'funding',
  trans_from_exchange: 'transfer',
  trans_to_exchange: 'transfer',
  transfer_in: 'transfer',
  transfer_out: 'transfer',
};

export const mapBitgetBill = (row: Record<string, any>): IncomeRecord[] => {
  const timestamp = parseInt(String(row.cTime ?? '0'), 10);
  if (!Number.isFinite(timestamp) || timestamp <= 0) return [];

  const type = String(row.businessType ?? '');
  if (!type) return [];

  // SUFFIXED per emitted record, matching okx/bybit/bitunix/hyperliquid. A bill yields both
  // a realized-PnL and a commission record, so the bare `billId` is NOT unique per record —
  // and any consumer deduping on `tranId` silently keeps one of the two. That is not
  // hypothetical: the shared income pager did exactly that here, discarding the commission
  // from every trade (820 records where the row census predicted ~1190). The pager's key is
  // now composite as well, but disambiguating at the SOURCE is what every other venue does
  // and is what makes the record self-describing.
  const billId = String(row.billId ?? '');
  const asset = String(row.coin || 'USDT');
  const symbol = String(row.symbol ?? '');
  const amount = num(row.amount);
  const fee = num(row.fee);

  const base = { symbol, incomeType: type, asset, timestamp };
  const records: IncomeRecord[] = [];

  if (TRADE_TYPES.has(type)) {
    // GROSS. Emitting `amount + fee` here would understate every closing row by its
    // commission while the account total still reconciled — the shape this epic keeps
    // finding, and the reason the balance-delta oracle was used instead of an assumption.
    if (amount !== 0) {
      records.push({ ...base, tranId: `${billId}:pnl`, income: amount, category: 'realized_pnl' });
    }
    if (fee !== 0) {
      // Passed through UNCHANGED. Bitget already signs it negative when charged, matching
      // every other venue's commission sign, so negating it would double the error rather
      // than fix it.
      records.push({ ...base, tranId: `${billId}:fee`, incomeType: `${type}_fee`, income: fee, category: 'commission' });
    }
    return records;
  }

  const category = CATEGORY_BY_TYPE[type] ?? 'other';
  if (amount !== 0 || fee !== 0) {
    records.push({ ...base, tranId: `${billId}:${category}`, income: amount + fee, category });
  }

  return records;
};

export const mapBitgetBills = (rows: Array<Record<string, any>>): IncomeRecord[] =>
  rows.flatMap((row) => mapBitgetBill(row));
