import { describe, expect, it } from 'vitest';

import type { OemsReadResult } from './prompt';

import { boundReadResult, boundReadResults, OEMS_READ_RESULT_LIMITS } from './boundContext';

const candles = (n: number) =>
  Array.from({ length: n }, (_, i) => ({ timestamp: i, open: 1, high: 2, low: 0, close: 1.5, volume: 10 }));

describe('boundReadResult', () => {
  /** `Market.info` is typed `any` and holds the venue's raw response — the
   *  largest single contributor to a result and the likeliest to carry
   *  venue-authored prose back into a prompt. */
  it('drops `info` wherever it appears, at any depth', () => {
    const { data } = boundReadResult('getMarkets', [
      { symbol: 'BTCUSDT', info: { huge: 'x'.repeat(5_000) }, limits: { amount: { min: 1 }, info: 'nested' } },
    ]);
    expect(JSON.stringify(data)).not.toContain('huge');
    expect(JSON.stringify(data)).not.toContain('nested');
    expect(JSON.stringify(data)).toContain('BTCUSDT');
  });

  it('keeps the MOST RECENT candles, not the oldest', () => {
    const { data, truncated } = boundReadResult('getCandles', candles(500));
    const kept = data as { timestamp: number }[];
    expect(kept).toHaveLength(OEMS_READ_RESULT_LIMITS.candles);
    expect(kept[kept.length - 1].timestamp).toBe(499);
    expect(truncated).toBe(true);
  });

  it('leaves a short series alone and says so', () => {
    const { data, truncated } = boundReadResult('getCandles', candles(10));
    expect(data).toHaveLength(10);
    expect(truncated).toBe(false);
  });

  /** A book is sorted from the touch, and the touch is what matters. */
  it('keeps the levels NEAREST the touch on an order book', () => {
    const side = Array.from({ length: 100 }, (_, i) => ({ price: i, amount: 1, total: 1 }));
    const { data, truncated } = boundReadResult('getOrderbook', { bids: side, asks: side });
    const book = data as { bids: { price: number }[]; asks: { price: number }[] };
    expect(book.bids).toHaveLength(OEMS_READ_RESULT_LIMITS.orderbookLevels);
    expect(book.bids[0].price).toBe(0);
    expect(truncated).toBe(true);
  });

  it('caps an ordinary row list', () => {
    const { data } = boundReadResult('getMarkets', Array.from({ length: 500 }, (_, i) => ({ symbol: `S${i}` })));
    expect(data).toHaveLength(OEMS_READ_RESULT_LIMITS.rows);
  });

  /** Halving keeps the value VALID JSON. Slicing the serialized string would
   *  hand the model a broken object and invite it to guess the rest. */
  it('falls back to halving until it fits, and stays parseable', () => {
    const fat = Array.from({ length: 60 }, () => ({ blob: 'x'.repeat(2_000) }));
    const { data, truncated } = boundReadResult('getMarkets', fat);
    const json = JSON.stringify(data);
    expect(json.length).toBeLessThanOrEqual(OEMS_READ_RESULT_LIMITS.chars);
    expect(() => JSON.parse(json)).not.toThrow();
    expect(truncated).toBe(true);
  });

  /** A venue payload is not ours and may be shaped any way at all. Returning
   *  the original object at the depth cap re-admitted a self-reference, which
   *  then threw out of JSON.stringify and killed the turn. */
  it('survives a self-referencing result instead of throwing', () => {
    const cyclic: Record<string, unknown> = { symbol: 'BTCUSDT' };
    cyclic.self = cyclic;
    const { data } = boundReadResult('getMarkets', [cyclic]);
    expect(() => JSON.stringify(data)).not.toThrow();
    expect(JSON.stringify(data)).toContain('BTCUSDT');
  });

  it('survives a cyclic result inside the cumulative bound too', () => {
    const cyclic: Record<string, unknown> = { symbol: 'ETHUSDT' };
    cyclic.self = cyclic;
    expect(() => JSON.stringify(boundReadResults([result('getMarkets', [cyclic])]))).not.toThrow();
  });

  /** Gating the backstop on `Array.isArray` left an object result — a book, a
   *  balance, a market — with no character cap at all. */
  it('caps an oversized OBJECT result, which has nothing to halve', () => {
    const { data, truncated } = boundReadResult('getBalance', { blob: 'x'.repeat(40_000) });
    expect(data).toBe('[result too large to show]');
    expect(truncated).toBe(true);
  });

  it('leaves an ordinary object result alone', () => {
    const balance = { total: 100, free: 90, used: 10, upnl: 0 };
    expect(boundReadResult('getBalance', balance)).toEqual({ data: balance, truncated: false });
  });

  it('passes a scalar through untouched', () => {
    expect(boundReadResult('getActiveAccount', 'main')).toEqual({ data: 'main', truncated: false });
  });

  it('survives a null result', () => {
    expect(boundReadResult('getTicker', null)).toEqual({ data: null, truncated: false });
  });
});

const result = (tool: string, data: unknown): OemsReadResult => ({ tool, args: {}, ok: true, data });

/**
 * Results ACCUMULATE across hops — four hops of six reads is twenty-four
 * payloads in one prompt — so a per-result cap alone bounds nothing that
 * matters.
 */
describe('boundReadResults', () => {
  it('keeps a small turn intact', () => {
    const results = [result('getBalance', { total: 1 }), result('getTicker', { last: 2 })];
    expect(boundReadResults(results)).toHaveLength(2);
  });

  it('caps how many results a turn may carry at all', () => {
    const many = Array.from({ length: 30 }, (_, i) => result('getBalance', { i }));
    expect(boundReadResults(many)).toHaveLength(OEMS_READ_RESULT_LIMITS.results);
  });

  it('stays under the cumulative character cap', () => {
    const fat = Array.from({ length: 10 }, () => result('getMarkets', [{ blob: 'x'.repeat(4_000) }]));
    const bounded = boundReadResults(fat);
    expect(JSON.stringify(bounded).length).toBeLessThanOrEqual(OEMS_READ_RESULT_LIMITS.totalChars * 1.2);
  });

  /** Dropped, not vanished: the model must not re-request what it already
   *  spent a hop on. */
  it('replaces a dropped result with a note rather than removing it', () => {
    const fat = Array.from({ length: 10 }, () => result('getMarkets', [{ blob: 'x'.repeat(4_000) }]));
    const bounded = boundReadResults(fat);
    expect(bounded).toHaveLength(10);
    expect(bounded.some((entry) => entry.error?.includes('dropped'))).toBe(true);
  });

  it('keeps the NEWEST result when it has to drop', () => {
    const fat = [
      ...Array.from({ length: 9 }, () => result('getMarkets', [{ blob: 'x'.repeat(4_000) }])),
      result('getTicker', { last: 42 }),
    ];
    const bounded = boundReadResults(fat);
    expect(bounded[bounded.length - 1]).toMatchObject({ tool: 'getTicker', ok: true });
  });

  it('bounds each surviving result too, not just the list', () => {
    const [only] = boundReadResults([
      result(
        'getCandles',
        Array.from({ length: 900 }, (_, i) => ({ timestamp: i, open: 1, high: 1, low: 1, close: 1, volume: 1 })),
      ),
    ]);
    expect((only.data as unknown[]).length).toBeLessThanOrEqual(OEMS_READ_RESULT_LIMITS.candles);
  });

  it('leaves a failed result alone', () => {
    const failed: OemsReadResult = { tool: 'getCandles', args: {}, ok: false, error: 'venue down' };
    expect(boundReadResults([failed])).toEqual([failed]);
  });
});

describe('a failed read is bounded too', () => {
  it('clips a venue error string', () => {
    const [only] = boundReadResults([
      { tool: 'getCandles', args: {}, ok: false, error: 'e'.repeat(5_000) },
    ]);
    expect(only.error?.length).toBeLessThanOrEqual(OEMS_READ_RESULT_LIMITS.errorChars);
  });
});


/**
 * The counts, and the merge that keeps them alive across the second pass.
 *
 * A result is bounded in the browser (so the body fits) and again in the route
 * (so the prompt fits). By the second pass the excess is already gone, so a
 * pass that reported only its own findings would answer "not truncated" and
 * erase the record.
 */
describe('truncation counts survive re-bounding', () => {
  it('reports how many of how many were kept', () => {
    const raw = Array.from({ length: 900 }, (_, index) => ({ timestamp: index }));
    expect(boundReadResult('getCandles', raw)).toMatchObject({ truncated: true, kept: 200, total: 900 });
  });

  it('counts both sides of a book together', () => {
    const book = {
      bids: Array.from({ length: 500 }, (_, index) => [index, 1]),
      asks: Array.from({ length: 400 }, (_, index) => [index, 1]),
    };
    expect(boundReadResult('getOrderbook', book)).toMatchObject({ truncated: true, kept: 40, total: 900 });
  });

  it('leaves the counts off when nothing was dropped', () => {
    const result = boundReadResult('getTicker', { last: 1 });
    expect(result.truncated).toBe(false);
    expect(result.kept).toBeUndefined();
    expect(result.total).toBeUndefined();
  });

  it('keeps the flag when the second pass has nothing left to drop', () => {
    const alreadyBounded = Array.from({ length: 200 }, (_, index) => ({ timestamp: index }));
    const [again] = boundReadResults([
      { tool: 'getCandles', args: {}, ok: true, data: alreadyBounded, truncated: true, kept: 200, total: 900 },
    ]);
    // `total` is the FIRST pass's figure — what the venue returned — because
    // the second pass can no longer see it.
    expect(again).toMatchObject({ truncated: true, kept: 200, total: 900 });
  });

  it('reports the last pass kept count when the second pass drops more', () => {
    const rows = Array.from({ length: 60 }, () => ({ pad: 'x'.repeat(800) }));
    const [again] = boundReadResults([
      { tool: 'getMarkets', args: {}, ok: true, data: rows, truncated: true, kept: 60, total: 612 },
    ]);
    expect(again.truncated).toBe(true);
    expect(again.total).toBe(612);
    expect(again.kept).toBeLessThan(60);
  });
});

/**
 * The shape comes from the manifest, not from a method name written into this
 * package. That is the whole point of the field: renaming a read used to
 * update the manifest, the agent tool list, the prompt block, the CLI registry
 * and every drift test while silently dropping the cap here.
 */
describe('caps come from the manifest, not from string matching', () => {
  const book = (levels: number) => ({
    bids: Array.from({ length: levels }, (_, index) => [index, 1]),
    asks: Array.from({ length: levels }, (_, index) => [index, 1]),
  });

  it('caps a book by the shape the manifest declares', () => {
    const { data } = boundReadResult('getOrderbook', book(500));
    expect((data as { bids: unknown[] }).bids).toHaveLength(OEMS_READ_RESULT_LIMITS.orderbookLevels);
  });

  it('gives candles the candle allowance and every other list the row one', () => {
    expect((boundReadResult('getCandles', candles(900)).data as unknown[]).length).toBe(
      OEMS_READ_RESULT_LIMITS.candles,
    );
    const rows = Array.from({ length: 900 }, (_, index) => ({ symbol: `S${index}` }));
    expect((boundReadResult('getMarkets', rows).data as unknown[]).length).toBe(OEMS_READ_RESULT_LIMITS.rows);
  });

  /**
   * `boundReadResults` runs in the route BEFORE anything validates the tool
   * name, so a forged body lands here. Throwing would turn it into a 500; the
   * value's own shape has to answer instead, and a book must still be capped.
   */
  it('still caps a book sent under a tool name it does not know', () => {
    const { data, truncated } = boundReadResult('not-a-read', book(500));
    expect(truncated).toBe(true);
    expect((data as { bids: unknown[] }).bids).toHaveLength(OEMS_READ_RESULT_LIMITS.orderbookLevels);
  });

  it('treats an unknown tool returning a list as rows, the tighter cap', () => {
    const rows = Array.from({ length: 900 }, (_, index) => ({ symbol: `S${index}` }));
    expect((boundReadResult('not-a-read', rows).data as unknown[]).length).toBe(OEMS_READ_RESULT_LIMITS.rows);
  });
});

/**
 * `isBook` uses `Object.hasOwn` rather than `in`. `in` walks the prototype
 * chain, so a polluted `Object.prototype.bids` would classify every object
 * result as a book — and on the unknown-tool path that REPLACES the payload
 * with `{bids, asks}` drawn off the prototype.
 */
describe('shape detection does not read the prototype chain', () => {
  it('leaves a plain object alone under a polluted Object.prototype', () => {
    (Object.prototype as unknown as { bids?: unknown[] }).bids = [1, 2, 3];
    try {
      expect(boundReadResult('not-a-read', { last: 62_000 }).data).toEqual({ last: 62_000 });
    } finally {
      delete (Object.prototype as unknown as { bids?: unknown[] }).bids;
    }
  });
});

/**
 * `isBook` proves the KEY is present, not that its value is a list. A venue
 * answering `{bids: 5}`, or a forged body naming any tool at all, used to
 * reach `.slice` on a number — and `readBody` sits inside the route's
 * `JSON.parse` try, so the raise surfaced as "Request body must be JSON".
 */
describe('a malformed book is bounded, not thrown', () => {
  it('survives bids that are not a list', () => {
    expect(() => boundReadResult('getOrderbook', { bids: 5, asks: null })).not.toThrow();
    expect(boundReadResult('getOrderbook', { bids: 5, asks: null }).data).toEqual({ bids: [], asks: [] });
  });

  it('survives the same under a tool name it does not know', () => {
    expect(() => boundReadResult('not-a-read', { asks: 'nope' })).not.toThrow();
  });
});
