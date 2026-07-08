import { describe, expect, it } from 'vitest';

import { findTickerMarkForCurrency } from './okx.exchange';

// OKX tickers carry both an instId `id` (e.g. `ETH-USDT-SWAP`) and a compact
// `symbol` (e.g. `ETHUSDT`, dashes stripped — see `parseTickers`/`fetchMarkets`).
// `formatBalanceUsedAvail` converts each balance currency's equity to USD
// notional by looking up that currency's mark price. The old code used
// `tickers.find((t) => t.symbol.startsWith(ccy))`, which returns the FIRST
// prefix match — so `ETH` could resolve to `ETHFI...` and `SOL` to `SOLO...`,
// multiplying equity by an unrelated instrument's mark.
describe('findTickerMarkForCurrency', () => {
  it('matches the exact instrument, not a prefix collision (ETH vs ETHFI)', () => {
    const tickers = [
      { id: 'ETHFI-USDT-SWAP', symbol: 'ETHFIUSDT', mark: 3 },
      { id: 'ETH-USDT-SWAP', symbol: 'ETHUSDT', mark: 3000 },
    ];

    // Must pick the exact ETH instrument (3000), NOT the ETHFI prefix hit (3).
    expect(findTickerMarkForCurrency(tickers, 'ETH')).toBe(3000);
  });

  it('does not collide SOL with SOLO', () => {
    const tickers = [
      { id: 'SOLO-USDT-SWAP', symbol: 'SOLOUSDT', mark: 0.5 },
      { id: 'SOL-USDT-SWAP', symbol: 'SOLUSDT', mark: 150 },
    ];

    expect(findTickerMarkForCurrency(tickers, 'SOL')).toBe(150);
  });

  it('falls back to 1 for a stablecoin with no linear instrument', () => {
    const tickers = [{ id: 'BTC-USDT-SWAP', symbol: 'BTCUSDT', mark: 60000 }];

    expect(findTickerMarkForCurrency(tickers, 'USDT')).toBe(1);
  });

  it('falls back to 1 when no ticker exists for the currency', () => {
    const tickers = [{ id: 'BTC-USDT-SWAP', symbol: 'BTCUSDT', mark: 60000 }];

    expect(findTickerMarkForCurrency(tickers, 'FOO')).toBe(1);
  });
});
