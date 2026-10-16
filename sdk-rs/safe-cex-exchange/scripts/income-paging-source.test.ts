import { afterAll, expect, it, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';

const capture = vi.hoisted(() => {
  const encode = (value: any): any => {
    if (typeof value === 'number') {
      const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
      return { numberBits: Number.isNaN(value) ? '7ff8000000000000' : bytes.toString('hex') };
    }
    if (Array.isArray(value)) return value.map(encode);
    if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, encode(v)]));
    return value;
  };
  return { encode, cases: [] as any[], helpers: [] as any[], boundaries: {} as any };
});

vi.mock('../../../packages/safe-cex/src/exchanges/income-paging', async importOriginal => {
  const actual: any = await importOriginal();
  return { ...actual, paginateIncomeRange: async (fetch: any, options: any) => {
    const row: any = { name: expect.getState().currentTestName, options: capture.encode(options), pages: [], events: [] };
    capture.cases.push(row);
    const timer = globalThis.setTimeout;
    globalThis.setTimeout = ((callback: any, ms: any, ...args: any[]) => {
      row.events.push({ delay: capture.encode(ms) });
      return timer(callback, ms, ...args);
    }) as typeof setTimeout;
    try {
    const result = await actual.paginateIncomeRange(async (params: any) => {
      row.events.push({ fetch: row.pages.length });
      const page: any = { params: capture.encode(params) }; row.pages.push(page);
      try { const result = await fetch(params); page.records = capture.encode(result); return result; }
      catch (error) { page.error = true; throw error; }
    }, options);
    row.result = capture.encode(result);
    return result;
    } finally { globalThis.setTimeout = timer; }
  } };
});

import '../../../packages/safe-cex/src/__tests__/audit-regressions/income-paging.test';
import { paginateIncomeRange } from '../../../packages/safe-cex/src/exchanges/income-paging';

const record = (timestamp: number, tranId = 'transaction', income = 17.25) => ({
  symbol: 'symbol-A', incomeType: 'type-B', income, asset: 'asset-C', timestamp, tranId,
  tradeId: 'trade-D', category: 'funding' as const,
});

it('supplement: native field identity, ties, first identity wins and all category spellings', async () => {
  const actual: any = await vi.importActual('../../../packages/safe-cex/src/exchanges/income-paging');
  const categories = [undefined, 'realized_pnl', 'funding', 'commission', 'rebate', 'transfer', 'liquidation', 'other'];
  for (const category of categories) for (const tranId of ['', 'transaction|A']) {
    for (const n of [-0, 0, 17.25, -23.5, Infinity, -Infinity, NaN]) {
      const row = { ...record(n, tranId, n), category };
      capture.helpers.push({ kind: 'identify', row: capture.encode(row), expected: actual.identify(row) });
    }
  }
  for (const timestamp of [-0, 13.25, -29.5, Infinity, NaN]) {
    const row = record(timestamp, '', 37.75);
    capture.helpers.push({ kind: 'identify', row: capture.encode(row), expected: actual.identify(row) });
  }
  for (const tranId of ['é', 'İ', '💰', '\ufeff', '\u0085']) {
    const row = record(13.25, tranId, 37.75);
    capture.helpers.push({ kind: 'identify', row: capture.encode(row), expected: actual.identify(row) });
  }
  for (const times of [[], [1], [1, 2], [2, 1], [-0, 0], [1, 3, 2], [NaN, 1, 2, Infinity], [-1, 3, 2, 5]]) {
    const page = times.map(t => record(t));
    capture.helpers.push({ kind: 'direction', page: capture.encode(page), expected: actual.pageDirection(page, 0, 4) });
  }
  const first = record(-0, 'same', 17.25), duplicate = { ...first, asset: 'replaced', tradeId: 'replaced' };
  const second = record(0, 'other', -23.5);
  let calls = 0;
  const result = await paginateIncomeRange(async () => ++calls === 1 ? [first, duplicate, second] : [], {
    startTime: -1.5, endTime: 4.25, chunkMs: 9.5, pageLimit: 7.5, maxPagesPerChunk: 11.5,
  });
  expect(result.records[0]).toBe(first);
  expect(result.records[1]).toBe(second);
  expect(Object.is(result.records[0].timestamp, -0)).toBe(true);
  expect(result.records[0].asset).toBe('asset-C');
  expect(result.records[0].tradeId).toBe('trade-D');
  const aliased = result.records[0]; first.asset = 'later';
  capture.boundaries.alias = { same: aliased === first, consumerAsset: aliased.asset };
  const surrogate = { ...record(1), tranId: '\ud800' };
  capture.boundaries.surrogate = { input: JSON.stringify(surrogate.tranId), output: JSON.stringify(actual.identify(surrogate)) };
  capture.boundaries.nullIdentity = actual.identify(null);
});

it('supplement: budgets and page length count malformed numeric rows independently', async () => {
  for (const budget of [0, -1, 1, 2.5, NaN]) {
    let calls = 0;
    await paginateIncomeRange(async () => ++calls <= 2 ? [record(2), record(NaN)] : [], {
      startTime: 1, endTime: 5, chunkMs: 8, pageLimit: 2, maxPagesPerChunk: budget,
    });
  }
});

it('supplement: native numeric options and global bounds', async () => {
  const options = [
    { startTime: NaN, endTime: 3, chunkMs: 7, pageLimit: 11 },
    { startTime: 1, endTime: NaN, chunkMs: 7, pageLimit: 11 },
    { startTime: 1, endTime: 3, chunkMs: NaN, pageLimit: 11 },
    { startTime: 1, endTime: 3, chunkMs: 7, pageLimit: NaN },
    { startTime: -0, endTime: 0, chunkMs: 7, pageLimit: 11 },
    { startTime: 1, endTime: 3, chunkMs: Infinity, pageLimit: 11 },
    { startTime: -Infinity, endTime: 3, chunkMs: Infinity, pageLimit: 11 },
    { startTime: 1, endTime: Infinity, chunkMs: Infinity, pageLimit: Infinity },
  ];
  for (const opts of options) await paginateIncomeRange(async () => [], opts);
  let index = 0;
  const pages = [[record(5), record(4)], [record(8, 'outside'), record(2, 'inside-first'), record(3, 'inside-second')]];
  await paginateIncomeRange(async () => pages[index++] ?? [], {
    startTime: 1, endTime: 10, chunkMs: 20, pageLimit: 3, maxPagesPerChunk: 10,
  });
});

afterAll(() => {
  const bytes = readFileSync(new URL('../../../packages/safe-cex/src/__tests__/audit-regressions/income-paging.test.ts', import.meta.url));
  const hash = createHash('sha256').update(bytes).digest('hex');
  expect(hash, 'unchanged shared feeder SHA256').toBe('676b632262daa2632a202dc89dd91d332cef8285160fea9328fce9f35833d248');
  expect(bytes.length, 'unchanged shared feeder bytes').toBe(17508);
  const destination = process.env.SAFE_CEX_INCOME_CAPTURE;
  expect(destination, 'PAGER parent capture destination').toBeTruthy();
  writeFileSync(destination!, JSON.stringify({
    provenance: { architecture: process.arch, node: process.version, feederBytes: bytes.length, feederSha256: hash },
    ...capture,
  }) + '\n');
});
