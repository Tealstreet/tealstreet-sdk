import type { IncomeRecord } from '../../types';

/**
 * BitMEX `/user/walletHistory` -> `IncomeRecord[]`.
 *
 * BitMEX had NO income axis at all before this: `fetchIncomeHistory` fell through to the
 * base stub returning `[]`, which made the venue impossible to grade — there was no second
 * axis for the closed-vs-ledger identity to compare against.
 *
 * **EVERY MONETARY VALUE IS IN MINOR UNITS, AND THE SCALE IS PER CURRENCY.** Measured live
 * (sheet row 43): `walletBalance: 60827952` in `USDt` on an account worth about sixty
 * dollars — a factor of 1e6 — while `XBt` rows on the SAME response are satoshis, 1e8. A
 * single hardcoded divisor is therefore wrong for half the rows, and summing the raw
 * `amount` across currencies (as a naive reader would) produces a number with no meaning:
 * the measured 200-row window summed to `-1826112` across mixed `USDt` and `XBt`.
 *
 * The scale comes from the venue's own `/wallet/assets` (already fetched into
 * `exchange.currencies` as a precision), not from a constant here. A currency whose scale is
 * unknown is DROPPED rather than guessed — a wrong power of ten reconciles against a ledger
 * scaled the same way and passes every invariant except the magnitude check.
 *
 * **`amount` is NET of `fee`.** Pinned by rows where the trade broke even and the entire
 * loss is the commission: `amount: -6594` against `fee: 6594`. So gross realized PnL is
 * `amount + fee`, and the two are emitted as SEPARATE records — a realized-PnL record and a
 * commission record — matching how every other venue's ledger decomposes.
 *
 * **The symbol is in `address`**, not in a `symbol` field. It is the instrument for trading
 * rows and a wallet address for transfers, so it is only read for the row types that trade.
 */

const num = (value: unknown) => {
  const parsed = typeof value === 'number' ? value : Number.parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

/** `transactType` -> our normalized category. Anything unlisted is deliberately dropped. */
const CATEGORY_BY_TYPE: Record<string, IncomeRecord['category']> = {
  RealisedPNL: 'realized_pnl',
  Deposit: 'transfer',
  Withdrawal: 'transfer',
  Transfer: 'transfer',
  AffiliatePayout: 'other',
};

export type BitmexIncomeDeps = {
  /**
   * Minor units per major unit for a currency — `1e6` for `USDt`, `1e8` for `XBt`.
   *
   * Supplied from the venue's own `/wallet/assets` scales rather than hardcoded, and
   * returning `undefined` DROPS the row. Guessing here is the single most dangerous thing
   * this file could do.
   */
  scaleFor: (currency: string) => number | undefined;
};

export const mapBitmexWalletHistory = (rows: Array<Record<string, any>>, deps: BitmexIncomeDeps): IncomeRecord[] => {
  const records: IncomeRecord[] = [];

  for (const row of rows) {
    // Only completed movements are real money. A pending withdrawal has not left the
    // account and counting it would make the ledger disagree with the balance.
    if (row.transactStatus && String(row.transactStatus) !== 'Completed') continue;

    const type = String(row.transactType ?? '');
    const category = CATEGORY_BY_TYPE[type];
    if (!category) continue;

    const currency = String(row.currency ?? '');
    const scale = deps.scaleFor(currency);
    // Unknown scale means unknown magnitude. Dropping is honest; dividing by a guess is a
    // silent order-of-magnitude error that reconciles against a same-scaled ledger.
    if (!scale || !Number.isFinite(scale) || scale <= 0) continue;

    const timestamp = Date.parse(String(row.transactTime ?? row.timestamp ?? ''));
    if (!Number.isFinite(timestamp)) continue;

    // `address` carries the instrument on trading rows and a wallet address on transfers.
    const symbol = category === 'realized_pnl' ? String(row.address ?? '') : '';
    // SUFFIXED per emitted record. A RealisedPNL row yields BOTH a realized-PnL and a
    // commission record, so the bare `transactID` is not unique per record and any consumer
    // deduping on `tranId` keeps only one of them — measured on bitget, where exactly that
    // discarded every commission. Five other venues already suffix; these were the outliers.
    const transactId = String(row.transactID ?? '');
    const asset = currency;

    const feeMinor = num(row.fee);
    const amountMinor = num(row.amount);

    if (category === 'realized_pnl') {
      // `amount` is NET of `fee`, so the gross PnL term is `amount + fee`. Emitting the net
      // figure as "realized PnL" would understate it by the commission on every row while
      // the total still reconciled — the exact shape this epic keeps finding.
      const gross = (amountMinor + feeMinor) / scale;
      if (gross !== 0) {
        records.push({
          symbol,
          incomeType: type,
          income: gross,
          asset,
          timestamp,
          tranId: `${transactId}:pnl`,
          category: 'realized_pnl',
        });
      }
      if (feeMinor !== 0) {
        records.push({
          symbol,
          incomeType: 'Commission',
          // Negative = paid, matching every other venue's commission sign.
          income: -feeMinor / scale,
          asset,
          timestamp,
          tranId: `${transactId}:fee`,
          category: 'commission',
        });
      }
      continue;
    }

    if (amountMinor !== 0) {
      records.push({
        symbol,
        incomeType: type,
        income: amountMinor / scale,
        asset,
        timestamp,
        tranId: `${transactId}:${category}`,
        category,
      });
    }
  }

  return records;
};
