import type { Execution } from '../../types';

import { OrderSide } from '../../types';

/**
 * Kraken Futures `GET /v3/fills` row -> `Execution`.
 *
 * Measured live against sheet row 65, not read from documentation. Fields:
 * `fill_id, symbol, side, order_id, cliOrdId, size, price, fillTime, fillType,
 * realized_pnl, realized_funding`.
 *
 * **There is NO fee on this route.** The fee lives only in `history/v3/account-log`, which
 * is what makes the two axes genuinely independent rather than two views of one payload —
 * the fills carry PnL and funding, the ledger carries PnL, funding AND the fee, and the
 * overlap is what can be reconciled.
 *
 * `fill_id` is the SAME value the ledger reports as `execution`, so the two axes join on it
 * directly. That join is the point of building this axis at all.
 */

/**
 * `fillTime` is ISO-8601, like the ledger's `date` and unlike every other venue on this axis.
 * `Execution.timestamp` is documented as SECONDS — not milliseconds, which is
 * `HistoricalTrade.time` — so this converts once, here, rather than at each call site.
 */
const toSeconds = (value: unknown): number => {
  const ms = Date.parse(String(value ?? ''));
  return Number.isFinite(ms) ? Math.floor(ms / 1000) : 0;
};

const num = (value: unknown): number => {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
};

/** Presence, not truthiness — a fill that realizes exactly 0 is real and common. */
const optionalNum = (value: unknown): number | undefined => {
  if (value === null || value === undefined || value === '') return undefined;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : undefined;
};

export const mapKrakenFill = (
  row: Record<string, any>,
  resolveSymbol: (venueSymbol: string) => string | undefined,
): Execution | null => {
  const id = String(row?.fill_id ?? '');
  const venueSymbol = String(row?.symbol ?? '');
  const timestamp = toSeconds(row?.fillTime);
  if (!id || !venueSymbol || !timestamp) return null;

  const side = String(row?.side ?? '').toLowerCase();
  if (side !== 'buy' && side !== 'sell') return null;

  return {
    id,
    timestamp,
    symbol: resolveSymbol(venueSymbol) ?? venueSymbol,
    orderId: row?.order_id ? String(row.order_id) : undefined,
    price: num(row?.price),
    qty: num(row?.size),
    side: side === 'buy' ? OrderSide.Buy : OrderSide.Sell,
    // NO `commission`: this route does not report one. Emitting 0 would assert the venue
    // charged nothing, and any reconciliation against the ledger — which DOES carry the fee
    // — would then be short by exactly the commission and look like a mapping defect.
    maker: String(row?.fillType ?? '').toLowerCase() === 'maker',
    // Kraken is one-way only (`PositionMode.OneWay`), and the route reports no leg, so this
    // is deliberately left absent rather than defaulted — absent means "not reported".
    realizedPnl: optionalNum(row?.realized_pnl),
  };
};

export const mapKrakenFills = (
  rows: Array<Record<string, any>>,
  resolveSymbol: (venueSymbol: string) => string | undefined,
): Execution[] =>
  rows.map((row) => mapKrakenFill(row, resolveSymbol)).filter((fill): fill is Execution => fill !== null);
