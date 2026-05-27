/**
 * Text-only PnL formatter exposed on api.utils.formatPnl.
 * See SPEC §2.1 + §2.7 FormatPnlOpts.
 *
 * Wraps safe-cex's `formatPnl(pnl, precision=2, prefix?)` with the spec's
 * options surface. The wrapper layers on `signed` (force a leading +/-) and
 * `abbreviated` (k/M/B suffix for large magnitudes) — neither is offered by
 * the underlying safe-cex helper. `currency` maps to the safe-cex `prefix`.
 */
import { formatPnl as safeFormatPnl } from '@tealstreet/safe-cex/src/utils/risk';
import type { FormatPnlOpts } from '../api/CoreApi.types';

const abbreviate = (value: number, precision: number): string => {
  const abs = Math.abs(value);
  if (abs >= 1e9) return `${(value / 1e9).toFixed(precision)}B`;
  if (abs >= 1e6) return `${(value / 1e6).toFixed(precision)}M`;
  if (abs >= 1e3) return `${(value / 1e3).toFixed(precision)}k`;
  return value.toFixed(precision);
};

export const formatPnl = (value: number, opts?: FormatPnlOpts): string => {
  const precision = opts?.precision ?? 2;
  const currency = opts?.currency;
  const signed = opts?.signed ?? false;
  const abbreviated = opts?.abbreviated ?? false;

  let body: string;
  if (abbreviated) {
    // `abbreviate` preserves the sign by dividing the signed value; the
    // currency prefix slots in after the leading `-` so negatives render
    // as `-$1.50k`, not `$-1.50k`.
    const abs = abbreviate(value, precision);
    if (currency) {
      body = value < 0 ? `-${currency}${abs.slice(1)}` : `${currency}${abs}`;
    } else {
      body = abs;
    }
  } else {
    body = safeFormatPnl(value, precision, currency);
  }

  if (signed && value > 0 && !body.startsWith('+') && !body.startsWith('-')) {
    return `+${body}`;
  }
  return body;
};
