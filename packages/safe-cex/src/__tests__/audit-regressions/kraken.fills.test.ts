import { describe, expect, it } from 'vitest';

import { mapKrakenFill, mapKrakenFills } from '../../exchanges/kraken/kraken.fills';

/**
 * Kraken Futures `/v3/fills` -> `Execution`.
 *
 * Rows transcribed from a live probe of sheet row 65. This is kraken's SECOND axis, and the
 * one that makes it gradeable: `fill_id` here is the same value the ledger reports as
 * `execution`, so the two independently-sourced axes join directly.
 */

const resolveSymbol = (venueSymbol: string) => (venueSymbol === 'PF_XRPUSD' ? 'XRPUSD' : undefined);

/** As returned with NO parameters — carries realized_pnl and realized_funding. */
const withPnl = {
  fill_id: 'd19f075f-78f4-4d3e-88b0-7f8b2a8ea287',
  symbol: 'PF_XRPUSD',
  side: 'buy',
  order_id: 'a27be3e2-5fd9-409d-94d7-644f982f1d2c',
  size: 1,
  price: 1.02248,
  fillTime: '2026-08-12T10:19:53.188Z',
  fillType: 'maker',
  realized_pnl: 0,
  realized_funding: -0.00014243935,
};

/**
 * As returned WITH `lastFillTime` — the same fill, two keys lighter.
 *
 * This is the venue's real behaviour, measured: 11 keys unparameterised, 9 with the window
 * applied. Supplying the window silently strips `realized_pnl` and `realized_funding`.
 */
const windowed = {
  fill_id: 'decfc338-0fb7-4336-81fd-94dd47242ca3',
  symbol: 'PF_DOGEUSD',
  side: 'buy',
  order_id: 'a27baf47-8f5b-4ef4-8fe2-2b0c7b90ffee',
  size: 28,
  price: 0.071533,
  fillTime: '2026-08-12T07:50:53.797Z',
  fillType: 'taker',
};

describe('mapKrakenFill', () => {
  it('maps a fill carrying PnL', () => {
    expect(mapKrakenFill(withPnl, resolveSymbol)).toMatchObject({
      id: 'd19f075f-78f4-4d3e-88b0-7f8b2a8ea287',
      symbol: 'XRPUSD',
      side: 'buy',
      qty: 1,
      price: 1.02248,
      maker: true,
      realizedPnl: 0,
    });
  });

  it('reports a genuine zero PnL as 0, not as absent', () => {
    // An opening fill realizes exactly 0 and the venue says so. Folding that to `undefined`
    // would make "broke even" indistinguishable from "this page cannot say" — and on this
    // venue the second case is real, because the windowed page drops the field entirely.
    expect(mapKrakenFill(withPnl, resolveSymbol)?.realizedPnl).toBe(0);
  });

  it('reports an ABSENT PnL as absent, which is what a windowed page returns', () => {
    expect(mapKrakenFill(windowed, resolveSymbol)?.realizedPnl).toBeUndefined();
  });

  it('never invents a commission — this route reports none', () => {
    // Emitting 0 would assert the venue charged nothing. The fee exists only in the ledger,
    // and a reconciliation against it would then be short by exactly the commission and read
    // as a mapping defect.
    expect(mapKrakenFill(withPnl, resolveSymbol)?.commission).toBeUndefined();
  });

  it('converts the ISO fillTime to SECONDS, matching Execution.timestamp', () => {
    // `Execution.timestamp` is seconds; `HistoricalTrade.time` is milliseconds. Getting this
    // backwards puts every fill 1000x into the future.
    expect(mapKrakenFill(withPnl, resolveSymbol)?.timestamp).toBe(Math.floor(Date.parse(withPnl.fillTime) / 1000));
  });

  it('falls back to the venue symbol when the market is unknown', () => {
    expect(mapKrakenFill(windowed, resolveSymbol)?.symbol).toBe('PF_DOGEUSD');
  });

  it('refuses a row with no id, no symbol or an unusable side', () => {
    expect(mapKrakenFill({ ...withPnl, fill_id: '' }, resolveSymbol)).toBeNull();
    expect(mapKrakenFill({ ...withPnl, side: 'unknown' }, resolveSymbol)).toBeNull();
    expect(mapKrakenFill({ ...withPnl, fillTime: 'not-a-date' }, resolveSymbol)).toBeNull();
  });

  it('drops unmappable rows from a page without dropping the rest', () => {
    const fills = mapKrakenFills([withPnl, { ...withPnl, fill_id: '' }, windowed], resolveSymbol);
    expect(fills).toHaveLength(2);
  });
});
