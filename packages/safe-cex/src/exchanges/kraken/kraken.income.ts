import type { IncomeRecord } from '../../types';

/**
 * Kraken Futures `GET /history/v3/account-log` row -> `IncomeRecord[]`.
 *
 * Schema measured live against sheet row 65 (see `CLAUDE.md` -> "MEASURED 2026-08-14"), not
 * taken from documentation — Kraken's own history pages 404 and the CCXT reference does not
 * carry a ledger parser.
 *
 * This is the richest ledger of any venue in this epic: `realized_pnl`, `realized_funding`
 * and `fee` arrive as SEPARATE fields on the same row, so none of them has to be inferred
 * from the others, and `old_balance` / `new_balance` give a running balance to pin the whole
 * thing against.
 */

/**
 * A `futures trade` books TWO rows sharing one `execution` id, and only one is money.
 *
 * The cash leg carries `asset: "usd"` with the fee and PnL. Its twin carries
 * `asset === contract` (`"pf_xrpusd"`), and its balance is a CONTRACT COUNT — measured
 * moving 195 -> 196 on a one-contract fill. Mapping that leg would not merely double-count
 * the trade, it would enter a position size into a currency total.
 */
const isPositionSizeLeg = (row: Record<string, any>) => {
  const asset = String(row?.asset ?? '').toLowerCase();
  const contract = String(row?.contract ?? '').toLowerCase();
  return asset.length > 0 && asset === contract;
};

/** `null` and `0` both occur and mean different things — see the mapper's PnL branch. */
const num = (value: unknown): number | undefined => {
  if (value === null || value === undefined || value === '') return undefined;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : undefined;
};

/**
 * ISO-8601 string, unlike every other venue on this axis, which report epoch millis.
 * Passing it through unparsed yields `NaN`/`0` and drops the whole account into 1970, where
 * a window filter then discards it — an empty axis rather than a visibly wrong date.
 */
const toMs = (value: unknown): number => {
  const parsed = Date.parse(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapKrakenAccountLogRow = (
  row: Record<string, any>,
  resolveSymbol: (contract: string) => string | undefined,
): IncomeRecord[] => {
  if (!row || isPositionSizeLeg(row)) return [];

  const timestamp = toMs(row.date);
  const bookingUid = String(row.booking_uid ?? row.id ?? '');
  if (!timestamp || !bookingUid) return [];

  const contract = String(row.contract ?? '');
  const info = String(row.info ?? 'unknown');
  const base = {
    // Absent on account-level rows (a transfer has no contract); the empty string keeps the
    // record rather than dropping value that still belongs in the account total.
    symbol: contract ? (resolveSymbol(contract) ?? contract) : '',
    incomeType: info,
    asset: String(row.asset ?? '').toUpperCase(),
    timestamp,
    tradeId: row.execution ? String(row.execution) : undefined,
  };

  const records: IncomeRecord[] = [];
  const realizedPnl = num(row.realized_pnl);
  const realizedFunding = num(row.realized_funding);
  const fee = num(row.fee);
  const liquidationFee = num(row.liquidation_fee);

  // `undefined` (the field was null) and `0` are BOTH real and mean different things: a
  // funding row reports `realized_pnl: null` because PnL is not its subject, while an
  // opening trade reports a genuine `0`. Testing presence rather than truthiness is what
  // keeps "not applicable" from printing as "broke even".
  if (realizedPnl !== undefined && realizedPnl !== 0) {
    records.push({ ...base, tranId: `${bookingUid}:pnl`, income: realizedPnl, category: 'realized_pnl' });
  }
  if (realizedFunding !== undefined && realizedFunding !== 0) {
    // Measured NEGATIVE when paid, which already matches the canonical sign. Passed through.
    records.push({
      ...base,
      tranId: `${bookingUid}:funding`,
      incomeType: `${info}_funding`,
      income: realizedFunding,
      category: 'funding',
    });
  }
  if (fee !== undefined && fee !== 0) {
    // NEGATED, deliberately. Kraken reports `fee` POSITIVE when charged (measured 0.0002,
    // 0.0198) — woo's sign and the OPPOSITE of okx and blofin, whose commission arrives
    // already negative and must be passed through untouched. Getting this backwards leaves
    // every account total reconciling while each fee is doubled in the wrong direction.
    records.push({
      ...base,
      tranId: `${bookingUid}:fee`,
      incomeType: `${info}_fee`,
      income: -fee,
      category: 'commission',
    });
  }
  if (liquidationFee !== undefined && liquidationFee !== 0) {
    records.push({
      ...base,
      tranId: `${bookingUid}:liquidation`,
      incomeType: `${info}_liquidation_fee`,
      income: -liquidationFee,
      category: 'liquidation',
    });
  }

  // A row that moved the balance but matched none of the branches above is still real value,
  // and the balance delta is the evidence. Never dropped.
  //
  // Classified rather than dumped into `other`: measured on the live account, the rows that
  // land here are `admin transfer` (95.07900000), `cross-exchange transfer` and `conversion`
  // — capital movements, not trading income. Leaving them as `other` put a 95-unit deposit
  // in the same bucket as an unclassified anomaly, which is exactly the kind of unexplained
  // row a total should never be allowed to absorb silently.
  if (records.length === 0) {
    const delta = (num(row.new_balance) ?? 0) - (num(row.old_balance) ?? 0);
    if (delta !== 0) {
      const category = /transfer|conversion|deposit|withdraw/i.test(info) ? 'transfer' : 'other';
      records.push({ ...base, tranId: `${bookingUid}:${category}`, income: delta, category });
    }
  }

  return records;
};

export const mapKrakenAccountLog = (
  rows: Array<Record<string, any>>,
  resolveSymbol: (contract: string) => string | undefined,
): IncomeRecord[] => rows.flatMap((row) => mapKrakenAccountLogRow(row, resolveSymbol));
