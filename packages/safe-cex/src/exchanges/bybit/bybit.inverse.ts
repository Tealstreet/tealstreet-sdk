import type { Position } from '../../types';

/**
 * `BTCUSD`, `ETHUSDZ26`. Checked on 2026-10-15 against instruments-info: all 26 inverse symbols match and none of
 * the 906 linear ones do. Read from the symbol so a position or order routes as inverse before its market loads.
 */
export const isBybitInverseSymbol = (symbol: string): boolean => /^[A-Z0-9]+USD([FGHJKMNQUVXZ]\d{2})?$/.test(symbol);

/**
 * An inverse position's `unrealizedPnl` is in the settle coin, as on `bybitinverse`. The unified balance is in
 * USD, so the coin amount is valued at the position row's mark before it joins the account sum.
 */
export const positionUpnlInQuote = (position: Position): number => {
  const upnl = position.unrealizedPnl || 0;
  if (!isBybitInverseSymbol(position.symbol)) return upnl;
  const mark = parseFloat(position.info?.markPrice ?? '');
  return upnl * (mark > 0 ? mark : position.entryPrice);
};
