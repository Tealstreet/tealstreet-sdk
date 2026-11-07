import { describe, expect, it } from 'vitest';

import {
  mapHyperliquidFillToIncomeRecords,
  mapHyperliquidFundingToIncomeRecord,
  paginateHyperliquidByTime,
} from '../reference/hyperliquid/hyperliquid.income';

const resolveSymbol = (coin: string) => (coin === 'DOGE' ? 'DOGE/USDC:USDC' : undefined);

const fill = (over: Record<string, any> = {}) => ({
  coin: 'DOGE',
  px: '0.1',
  sz: '100',
  side: 'B',
  time: 1754000000000,
  closedPnl: '1.25',
  fee: '0.0055',
  feeToken: 'USDC',
  tid: 'tid-1',
  hash: '0xabc',
  oid: 42,
  ...over,
});

describe('hyperliquid fill -> income', () => {
  it('emits the fee as well as the realized PnL', () => {
    // Hyperliquid has no separate fee row — the fee rides on the fill. Mapping
    // `closedPnl` alone drops every fee from the ledger.
    const records = mapHyperliquidFillToIncomeRecords(fill(), resolveSymbol);

    expect(records).toHaveLength(2);
    expect(records.map((r) => r.category).sort()).toEqual(['commission', 'realized_pnl']);
    expect(new Set(records.map((r) => r.tranId)).size).toBe(2);
  });

  it('reports a charged fee as negative income', () => {
    const commission = mapHyperliquidFillToIncomeRecords(fill(), resolveSymbol).find(
      (r) => r.category === 'commission',
    );
    expect(commission!.income).toBeCloseTo(-0.0055, 12);
  });

  it('treats a negative fee as a rebate, not an absolute-valued commission', () => {
    // `fetchHistoricalFills` in the same directory does Math.abs(fee), which is wrong for
    // maker rebates. This mapper must not copy that.
    const records = mapHyperliquidFillToIncomeRecords(fill({ fee: '-0.002', closedPnl: '0' }), resolveSymbol);

    expect(records).toHaveLength(1);
    expect(records[0].category).toBe('rebate');
    expect(records[0].income).toBeCloseTo(0.002, 12);
  });

  it('keys tranId on tid so identical partial fills do not collapse', () => {
    const a = mapHyperliquidFillToIncomeRecords(fill({ tid: 'tid-a' }), resolveSymbol);
    const b = mapHyperliquidFillToIncomeRecords(fill({ tid: 'tid-b' }), resolveSymbol);

    // Same coin, time, price, size and side — only the venue's own id differs.
    expect(a[0].tranId).not.toBe(b[0].tranId);
  });

  it('falls back to the raw coin rather than dropping an unresolvable market', () => {
    const records = mapHyperliquidFillToIncomeRecords(fill({ coin: 'NEWCOIN' }), resolveSymbol);
    expect(records[0].symbol).toBe('NEWCOIN');
  });
});

describe('hyperliquid funding -> income', () => {
  it('reads the NESTED REST row shape', () => {
    // REST `UserFundingEntry` nests under `delta`; the WS row is flat. A mapper written
    // against the flat shape yields undefined coin/usdc against REST rows and does not throw.
    const record = mapHyperliquidFundingToIncomeRecord(
      { time: 1754000000000, hash: '0xdef', delta: { coin: 'DOGE', usdc: '-0.31', fundingRate: '0.0001', szi: '100' } },
      resolveSymbol,
    );

    expect(record).not.toBeNull();
    expect(record!.symbol).toBe('DOGE/USDC:USDC');
    expect(record!.income).toBeCloseTo(-0.31, 12);
    expect(record!.category).toBe('funding');
  });

  it('also reads the FLAT websocket row shape', () => {
    const record = mapHyperliquidFundingToIncomeRecord(
      { time: 1754000000000, coin: 'DOGE', usdc: '0.12', fundingRate: '0.0001', szi: '100' },
      resolveSymbol,
    );

    expect(record!.income).toBeCloseTo(0.12, 12);
    expect(record!.symbol).toBe('DOGE/USDC:USDC');
  });

  it('drops a zero-value or malformed funding row', () => {
    expect(
      mapHyperliquidFundingToIncomeRecord({ time: 0, delta: { coin: 'DOGE', usdc: '1' } }, resolveSymbol),
    ).toBeNull();
    expect(
      mapHyperliquidFundingToIncomeRecord({ time: 1, delta: { coin: 'DOGE', usdc: '0' } }, resolveSymbol),
    ).toBeNull();
  });
});

describe('hyperliquid time pager', () => {
  const keyOf = (row: Record<string, any>) => String(row.tid);

  it('does NOT drop rows sharing the boundary millisecond', async () => {
    // The row cap can split a group of same-millisecond fills — one order sweeping
    // several levels produces exactly that. Advancing the cursor to `newest + 1` would
    // skip the remainder silently; an inclusive cursor plus id dedupe keeps them.
    const pages = [
      [
        { tid: 'a', time: 1000 },
        { tid: 'b', time: 2000 },
        { tid: 'c', time: 2000 },
      ],
      [
        { tid: 'c', time: 2000 },
        { tid: 'd', time: 2000 },
        { tid: 'e', time: 3000 },
      ],
      [{ tid: 'e', time: 3000 }],
    ];

    let call = 0;
    const post = async () => pages[Math.min(call++, pages.length - 1)];

    const rows = await paginateHyperliquidByTime(post, 'userFillsByTime', 0, 9999, keyOf);

    expect(rows.map((r) => r.tid)).toEqual(['a', 'b', 'c', 'd', 'e']);
  });

  it('terminates when a page is entirely overlap', async () => {
    const post = async () => [{ tid: 'a', time: 1000 }];
    const rows = await paginateHyperliquidByTime(post, 'userFillsByTime', 0, 9999, keyOf);

    expect(rows).toHaveLength(1);
  });

  it('reports when it stops at the page cap instead of stopping silently', async () => {
    let n = 0;
    const post = async () => [{ tid: `t${n++}`, time: 1000 + n }];
    const capHits: string[] = [];

    await paginateHyperliquidByTime(post, 'userFunding', 0, 9e12, keyOf, (type) => capHits.push(type), 3);

    expect(capHits).toEqual(['userFunding']);
  });

  it('stops cleanly on an empty first page', async () => {
    const rows = await paginateHyperliquidByTime(async () => [], 'userFunding', 0, 9999, keyOf);
    expect(rows).toEqual([]);
  });
});
