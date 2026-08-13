import { describe, expect, it } from 'vitest';

import { aggregateBybitClosedPnl } from '../../journal/bybit.position-adapter';
import { mapOkxPositionHistory } from '../../journal/okx.position-adapter';
import { PositionSide } from '../../types';

/**
 * Fixtures are the rows measured live on 2026-08-12 against e2e runs whose actions were
 * known independently (see `docs/POSITION_MODEL_VENUE_EVIDENCE_v1.md`), not invented
 * shapes. That matters: every trap this model exists for is a venue behaviour that looks
 * reasonable until you compare it against what actually happened.
 */

const okxDeps = {
  account: 'test',
  // DOGE-USDT-SWAP is 1000 DOGE per contract.
  toBase: (contracts: number) => contracts * 1000,
  resolveSymbol: (instId: string) => (instId === 'DOGE-USDT-SWAP' ? 'DOGEUSDT' : undefined),
  resolveSettleCurrency: () => 'USDT',
};

describe('okx position adapter', () => {
  // The measured partial-close ladder: opened 420 DOGE, closed in three 140s. OKX reported
  // ONE row for the round trip.
  const ladderRow = {
    instId: 'DOGE-USDT-SWAP',
    direction: 'long',
    posSide: 'net',
    posId: '111',
    cTime: '1786548503000',
    uTime: '1786548532000',
    closeTotalPos: '0.42',
    openAvgPx: '0.0708',
    closeAvgPx: '0.0708',
    realizedPnl: '-0.029736',
    fee: '-0.029736',
    fundingFee: '0',
    type: '1',
  };

  it('converts contracts to base units', () => {
    // 0.42 contracts is 420 DOGE. Emitting 0.42 would be wrong by the contract size with
    // nothing downstream able to notice.
    expect(mapOkxPositionHistory(ladderRow, okxDeps)?.closedSize).toBe(420);
  });

  it('reports realized PnL GROSS, adding back the fee OKX netted out', () => {
    const record = mapOkxPositionHistory(ladderRow, okxDeps)!;
    // OKX's -0.029736 is net of the 0.029736 fee, so gross is 0 — matching the fills axis,
    // whose three closes summed -0.0014 + 0.0014 + 0.
    expect(record.realizedPnl).toBeCloseTo(0, 10);
    expect(record.fees).toBeCloseTo(0.029736, 10);
  });

  it('treats one row as one round trip regardless of how many partials composed it', () => {
    // Three partial closes produced this single row, so the canonical close count is 1.
    expect(mapOkxPositionHistory(ladderRow, okxDeps)?.closeCount).toBe(1);
  });

  it('keeps a true open time', () => {
    const record = mapOkxPositionHistory(ladderRow, okxDeps)!;
    expect(record.openedAt).toBe(1786548503000);
    expect(record.closedAt).toBe(1786548532000);
    expect(record.closedAt! - record.openedAt!).toBeGreaterThan(0);
  });

  it('drops a row whose market is unknown rather than emitting the raw venue id', () => {
    expect(mapOkxPositionHistory({ ...ladderRow, instId: 'WAT-USDT-SWAP' }, okxDeps)).toBeNull();
  });

  it('classifies a liquidation as its own close reason', () => {
    expect(mapOkxPositionHistory({ ...ladderRow, type: '3' }, okxDeps)?.closeReason).toBe('liquidation');
    expect(mapOkxPositionHistory({ ...ladderRow, type: '5' }, okxDeps)?.closeReason).toBe('adl');
  });

  it('subtracts funding, which OKX also nets out of realizedPnl', () => {
    // Every earlier fixture carried fundingFee '0', so the funding term was unverified —
    // flipping its sign passed the whole suite. A position that PAID funding has a
    // negative fundingFee, and gross must differ from the net figure by that amount.
    const paidFunding = { ...ladderRow, realizedPnl: '-0.031736', fundingFee: '-0.002' };
    const record = mapOkxPositionHistory(paidFunding, okxDeps)!;
    expect(record.funding).toBeCloseTo(-0.002, 10);
    // -0.031736 + 0.029736 fees - (-0.002) funding = 0
    expect(record.realizedPnl).toBeCloseTo(0, 10);
  });

  it('treats a maker rebate as negative cost, not as a fee', () => {
    // OKX's `fee` is negative when charged, so a POSITIVE value is a rebate. Math.abs
    // here would report the rebate as a cost and add it with the wrong sign — an error of
    // twice the rebate that reconciles in aggregate and only appears on maker fills.
    const rebated = { ...ladderRow, realizedPnl: '0.001', fee: '0.001', fundingFee: '0' };
    const record = mapOkxPositionHistory(rebated, okxDeps)!;
    expect(record.fees).toBeCloseTo(-0.001, 10);
    expect(record.realizedPnl).toBeCloseTo(0, 10);
  });
});

describe('bybit position adapter', () => {
  // Resolver, not a constant: bybit's `linear` category spans USDT and USDC quotes, so one
  // response mixes settlement currencies and the row itself carries none.
  const deps = {
    account: 'test',
    resolveSettleCurrency: (symbol: string) => (symbol.endsWith('USDC') ? 'USDC' : 'USDT'),
  };

  // The SAME ladder on bybit produced THREE rows, one per partial close, each carrying the
  // position's entry price and that partial's exit.
  const ladderRows = [
    {
      symbol: 'DOGEUSDT',
      side: 'Sell',
      orderId: 'o1',
      closedSize: '141',
      avgEntryPrice: '0.07088',
      avgExitPrice: '0.07087',
      closedPnl: '-0.01240272',
      openFee: '0.00549636',
      closeFee: '0.00549636',
      createdTime: '1786548611000',
      updatedTime: '1786548611000',
      execType: 'Trade',
    },
    {
      symbol: 'DOGEUSDT',
      side: 'Sell',
      orderId: 'o2',
      closedSize: '141',
      avgEntryPrice: '0.07088',
      avgExitPrice: '0.07083',
      closedPnl: '-0.01803962',
      openFee: '0.00549481',
      closeFee: '0.00549481',
      createdTime: '1786548615000',
      updatedTime: '1786548615000',
      execType: 'Trade',
    },
    {
      symbol: 'DOGEUSDT',
      side: 'Sell',
      orderId: 'o3',
      closedSize: '141',
      avgEntryPrice: '0.07088',
      avgExitPrice: '0.07083',
      closedPnl: '-0.01803963',
      openFee: '0.00549481',
      closeFee: '0.00549482',
      createdTime: '1786548620000',
      updatedTime: '1786548620000',
      execType: 'Trade',
    },
  ];

  it('folds per-partial rows into one round trip', () => {
    const records = aggregateBybitClosedPnl(ladderRows, deps);
    // Without this fold a trade count reads 3 here and 1 on OKX for identical activity.
    expect(records).toHaveLength(1);
    expect(records[0].closeCount).toBe(3);
    expect(records[0].closedSize).toBe(423);
  });

  it('inverts the closing side to get the position direction', () => {
    // A `Sell` close means the position was long. Backwards flips every record.
    expect(aggregateBybitClosedPnl(ladderRows, deps)[0].side).toBe(PositionSide.Long);
  });

  it('reports realized PnL GROSS, adding back the fees bybit netted out', () => {
    const record = aggregateBybitClosedPnl(ladderRows, deps)[0];
    const netSum = -0.01240272 + -0.01803962 + -0.01803963;
    const feeSum = 0.00549636 + 0.00549636 + 0.00549481 + 0.00549481 + 0.00549481 + 0.00549482;
    expect(record.fees).toBeCloseTo(feeSum, 10);
    expect(record.realizedPnl).toBeCloseTo(netSum + feeSum, 10);
  });

  it('size-weights the exit price across the partial closes', () => {
    const record = aggregateBybitClosedPnl(ladderRows, deps)[0];
    expect(record.avgExitPrice).toBeCloseTo((141 * 0.07087 + 141 * 0.07083 + 141 * 0.07083) / 423, 10);
  });

  it('leaves openedAt absent, because bybit reports opened == closed', () => {
    // Forcing a value here would claim a zero hold time for every bybit trade.
    expect(aggregateBybitClosedPnl(ladderRows, deps)[0].openedAt).toBeUndefined();
  });

  it('does not merge runs with a different entry price', () => {
    // A different entry price is a different bucket, whatever the ordering.
    const reopened = [
      ...ladderRows,
      { ...ladderRows[0], orderId: 'o4', avgEntryPrice: '0.07200', updatedTime: '1786548999000' },
    ];
    expect(aggregateBybitClosedPnl(reopened, deps)).toHaveLength(2);
  });

  it('separates hedge legs even when the closes share everything else', () => {
    // A hedge flatten emits one row per leg; the leg comes from the ORDER axis because
    // bybit puts it on neither the fill nor the closed-PnL row.
    const legs: Record<string, PositionSide> = { o1: PositionSide.Long, o2: PositionSide.Short };
    const records = aggregateBybitClosedPnl(ladderRows.slice(0, 2), {
      ...deps,
      resolveLeg: (orderId) => legs[orderId],
    });
    expect(records).toHaveLength(2);
    expect(records.map((r) => r.positionSide).sort()).toEqual([PositionSide.Long, PositionSide.Short]);
    expect(records[0].sourceAxis).toBe('closed-positions+orders');
  });

  it('marks funding unattributed, because bybit books it against the symbol', () => {
    expect(aggregateBybitClosedPnl(ladderRows, deps)[0].fundingAttributed).toBe(false);
  });

  it('classifies liquidation and ADL from execType', () => {
    const bust = aggregateBybitClosedPnl([{ ...ladderRows[0], execType: 'BustTrade' }], deps);
    const adl = aggregateBybitClosedPnl([{ ...ladderRows[0], execType: 'AdlTrade' }], deps);
    expect(bust[0].closeReason).toBe('liquidation');
    expect(adl[0].closeReason).toBe('adl');
  });

  it('folds correctly when two symbols close interleaved', () => {
    // Bucketing must be by key, not by adjacency in a global sort. With DOGE@t1, BTC@t2,
    // DOGE@t3 an adjacency check yields three records and silently restores per-partial
    // grain — the one thing the fold exists to prevent.
    const interleaved = [
      ladderRows[0],
      { ...ladderRows[0], symbol: 'BTCUSDT', orderId: 'b1', updatedTime: '1786548613000' },
      ladderRows[1],
    ];
    const records = aggregateBybitClosedPnl(interleaved, deps);
    expect(records).toHaveLength(2);
    expect(records.find((r) => r.symbol === 'DOGEUSDT')!.closeCount).toBe(2);
  });

  it('does not split a round trip on an equivalent entry price written differently', () => {
    // Bybit returns decimal strings; "0.07088" and "0.070880" are the same price.
    const padded = [ladderRows[0], { ...ladderRows[1], avgEntryPrice: '0.070880' }];
    expect(aggregateBybitClosedPnl(padded, deps)).toHaveLength(1);
  });

  it('subtracts fills-derived funding to reach canonical gross', () => {
    // `closedPnl = realized - fees + funding` on bybit. The row carries no funding field,
    // so the term arrives from the fills axis via resolveContext. Measured consequence of
    // NOT doing this: a 30-day window missed the ledger by exactly its funding.
    const record = aggregateBybitClosedPnl(ladderRows.slice(0, 1), {
      ...deps,
      resolveContext: () => ({ openedAt: 1_786_548_000_000, funding: -0.004, fundingAttributed: true }),
    })[0];

    expect(record.funding).toBeCloseTo(-0.004, 10);
    expect(record.fundingAttributed).toBe(true);
    expect(record.openedAt).toBe(1_786_548_000_000);
    // netPnl + fees - funding, with funding NEGATIVE (paid), so gross is higher than the
    // fee-only figure. Getting this sign backwards moves the number by twice the funding.
    expect(record.realizedPnl).toBeCloseTo(-0.01240272 + 0.00549636 + 0.00549636 - -0.004, 10);
  });

  it('leaves funding embedded in realizedPnl when the fills axis cannot supply it', () => {
    // The honest degradation. No fills (or a position already open when the window began)
    // means no interval, so funding stays 0 and the figure remains net-of-funding —
    // which `--canonical` FAILs on rather than absorbing. Inventing a number here instead
    // would be a confident value built from an interval we could not see.
    const record = aggregateBybitClosedPnl(ladderRows.slice(0, 1), deps)[0];
    expect(record.funding).toBe(0);
    expect(record.fundingAttributed).toBe(false);
    // netPnl + openFee + closeFee, with no funding adjustment available.
    expect(record.realizedPnl).toBeCloseTo(-0.01240272 + 0.00549636 + 0.00549636, 10);
  });

  it('settles each symbol in its own currency within one response', () => {
    // Bybit's `linear` category covers USDT AND USDC perps, and the closed-PnL row carries
    // no currency of its own. Stamping one currency across the batch would mislabel every
    // USDC position — and would make a multi-currency check pass by construction, since
    // there is nothing in the data left to contradict it.
    const mixed = [ladderRows[0], { ...ladderRows[0], symbol: 'BTCPERP_USDC', orderId: 'u1' }];
    const records = aggregateBybitClosedPnl(mixed, deps);
    expect(records.find((r) => r.symbol === 'DOGEUSDT')!.settleCurrency).toBe('USDT');
    expect(records.find((r) => r.symbol === 'BTCPERP_USDC')!.settleCurrency).toBe('USDC');
  });
});

describe('cross-venue equivalence', () => {
  it('reports the same round trip identically from two disagreeing venue shapes', () => {
    // The whole point of the model: OKX hands us one row, bybit hands us three, and a
    // consumer must not be able to tell.
    const okx = mapOkxPositionHistory(
      {
        instId: 'DOGE-USDT-SWAP',
        direction: 'long',
        posSide: 'net',
        cTime: '1',
        uTime: '2',
        closeTotalPos: '0.423',
        openAvgPx: '0.07088',
        closeAvgPx: '0.070843',
        realizedPnl: '-0.01',
        fee: '-0.03',
        fundingFee: '0',
        type: '1',
      },
      okxDeps,
    )!;
    const bybit = aggregateBybitClosedPnl(
      [
        {
          symbol: 'DOGEUSDT',
          side: 'Sell',
          orderId: 'a',
          closedSize: '423',
          avgEntryPrice: '0.07088',
          avgExitPrice: '0.070843',
          closedPnl: '-0.01',
          openFee: '0.015',
          closeFee: '0.015',
          createdTime: '2',
          updatedTime: '2',
          execType: 'Trade',
        },
      ],
      { account: 'test', resolveSettleCurrency: () => 'USDT' },
    )[0];

    expect(okx.side).toBe(bybit.side);
    expect(okx.closedSize).toBeCloseTo(bybit.closedSize, 8);
    expect(okx.avgEntryPrice).toBeCloseTo(bybit.avgEntryPrice, 8);
    expect(okx.avgExitPrice!).toBeCloseTo(bybit.avgExitPrice!, 8);
    expect(okx.fees).toBeCloseTo(bybit.fees, 8);
    // Both gross, from a net field on one venue and a net field on the other.
    expect(okx.realizedPnl).toBeCloseTo(bybit.realizedPnl, 8);
  });
});
