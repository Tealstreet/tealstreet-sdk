import type { Market } from '../../types';

import { describe, expect, it } from 'vitest';

import { mapOkxClosedPosition } from '../../exchanges/okx/okx.closed-pnl';
import { PositionSide } from '../../types';

/** Row shape captured live from `/api/v5/account/positions-history` (sheet row 13). */
const row = (over: Record<string, any> = {}) => ({
  cTime: '1786523946912',
  ccy: 'USDT',
  closeAvgPx: '0.07168',
  closeTotalPos: '0.14',
  direction: 'long',
  fee: '-0.0100359',
  fundingFee: '0',
  instId: 'DOGE-USDT-SWAP',
  lever: '10.0',
  openAvgPx: '0.07169',
  pnl: '-0.0014',
  posId: '3816667956280692737',
  realizedPnl: '-0.0114359',
  uTime: '1786523975722',
  ...over,
});

const market = { symbol: 'DOGEUSDT', id: 'DOGE-USDT-SWAP' } as unknown as Market;
const contractsToBase = (contracts: number) => contracts * 1000;

describe('okx closed-position mapping', () => {
  it('carries realizedPnl, which is NET of both fee and funding', () => {
    // Measured live: pnl(-0.0014) + fee(-0.0100359) + fundingFee(0) = realizedPnl.
    // Bybit's closedPnl excludes funding, so a cross-check must know which convention a
    // venue uses or it reports a residual exactly the size of the funding term.
    const entry = mapOkxClosedPosition(row(), market, contractsToBase, 'acct')!;

    expect(entry.unrealizedPnl).toBeCloseTo(-0.0114359, 10);
    expect(entry.unrealizedPnl).toBeCloseTo(-0.0014 + -0.0100359 + 0, 10);
  });

  it('reports fees as paid, flipping okx negative-when-charged sign', () => {
    expect(mapOkxClosedPosition(row(), market, contractsToBase, 'acct')!.fees).toBeCloseTo(0.0100359, 10);
  });

  it('converts contracts to base units', () => {
    // Sizes here are CONTRACTS; a raw passthrough understates size by ctVal.
    const entry = mapOkxClosedPosition(row(), market, contractsToBase, 'acct')!;

    expect(entry.contracts).toBeCloseTo(140, 8);
    expect(entry.notional).toBeCloseTo(140 * 0.07168, 8);
  });

  it('maps direction to position side', () => {
    expect(mapOkxClosedPosition(row(), market, contractsToBase, 'acct')!.side).toBe(PositionSide.Long);
    expect(mapOkxClosedPosition(row({ direction: 'short' }), market, contractsToBase, 'acct')!.side).toBe(
      PositionSide.Short,
    );
  });

  it('stamps venue provenance, so it can cross-check the ledger', () => {
    expect(mapOkxClosedPosition(row(), market, contractsToBase, 'acct')!.provenance).toBe('venue');
  });

  it('uses cTime as open and uTime as close', () => {
    const entry = mapOkxClosedPosition(row(), market, contractsToBase, 'acct')!;

    expect(entry.time).toBe(1786523946912);
    expect(entry.closedTime).toBe(1786523975722);
  });

  it('falls back to the raw instId when the market is unknown', () => {
    expect(mapOkxClosedPosition(row(), undefined, contractsToBase, 'acct')!.symbol).toBe('DOGE-USDT-SWAP');
  });
});
