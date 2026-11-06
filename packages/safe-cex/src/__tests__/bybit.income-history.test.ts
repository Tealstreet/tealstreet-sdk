import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from './reference/bybit/bybit.exchange';
import { BybitInverseExchange } from './reference/bybit/bybitinverse.exchange';

/**
 * Characterization test for the LINEAR income path, written before that code was
 * extracted into `bybit.income.ts` so the extraction can be proved behaviour-
 * preserving. Nothing covered the HTTP path before this: the only income test
 * exercised `mapBybitTransactionToIncomeRecords` in isolation.
 *
 * Every assertion here is a documented invariant of the venue call, not a
 * preference — see the comments on each.
 */
const createStore = () =>
  ({
    subscribe: vi.fn(),
    markets: [],
    marketsBySymbol: {},
    tickersBySymbol: {},
    orders: [],
    positions: [],
  }) as any;

const createExchange = () => {
  const exchange = new BybitExchange(
    'bybit' as any,
    { key: 'key', secret: 'secret', testnet: false } as any,
    createStore(),
  );
  const get = vi.fn();
  exchange.xhr = { get } as any;
  exchange.log = vi.fn();
  return { exchange, get };
};

const row = (symbol: string, transactionTime: string) => ({
  symbol,
  category: 'linear',
  type: 'TRADE',
  transactionTime,
  cashFlow: '1',
  funding: '0',
  fee: '0',
  change: '1',
  currency: 'USDT',
  tradeId: `t-${symbol}-${transactionTime}`,
  orderId: `o-${symbol}-${transactionTime}`,
});

const okPage = (rows: any[], nextPageCursor?: string) => ({
  data: { retMsg: 'OK', retCode: 0, result: { list: rows, nextPageCursor } },
});

describe('bybit linear income history', () => {
  it('asks the transaction log for the UNIFIED account and the linear category', async () => {
    const { exchange, get } = createExchange();
    get.mockResolvedValueOnce(okPage([row('BTCUSDT', '1700000000000')]));

    await exchange.fetchIncomeHistory({ startTime: 1, endTime: 2 });

    const [endpoint, config] = get.mock.calls[0];
    expect(endpoint).toBe('/v5/account/transaction-log');
    expect(config.params.accountType).toBe('UNIFIED');
    expect(config.params.category).toBe('linear');
  });

  // V5 caps `limit` at 50 and REJECTS a larger one outright. Because a non-zero
  // retCode breaks with an empty list, an oversized limit would read downstream
  // as "this account has no ledger" rather than as an error.
  it('clamps limit to the venue maximum of 50', async () => {
    const { exchange, get } = createExchange();
    get.mockResolvedValueOnce(okPage([]));

    await exchange.fetchIncomeHistory({ limit: 500 });

    expect(get.mock.calls[0][1].params.limit).toBe(50);
  });

  it('maps a generic income type onto the venue transaction type', async () => {
    const { exchange, get } = createExchange();
    get.mockResolvedValueOnce(okPage([]));

    await exchange.fetchIncomeHistory({ incomeType: 'FUNDING_FEE' });

    expect(get.mock.calls[0][1].params.type).toBe('SETTLEMENT');
  });

  // A venue error yields an empty list rather than a throw. Pinned because the
  // caller cannot tell it apart from "no ledger", which is exactly why the limit
  // clamp above matters.
  it('returns empty and stops paging when the venue reports a non-zero retCode', async () => {
    const { exchange, get } = createExchange();
    get.mockResolvedValueOnce({ data: { retMsg: 'nope', retCode: 10001, result: {} } });

    await expect(exchange.fetchIncomeHistory({})).resolves.toEqual([]);
    expect(get).toHaveBeenCalledTimes(1);
  });

  it('follows the cursor across pages and stops when it is absent', async () => {
    const { exchange, get } = createExchange();
    get.mockResolvedValueOnce(okPage([row('BTCUSDT', '1700000000000')], 'cursor-1'));
    get.mockResolvedValueOnce(okPage([row('ETHUSDT', '1700000001000')]));

    const records = await exchange.fetchIncomeHistory({});

    expect(get).toHaveBeenCalledTimes(2);
    expect(get.mock.calls[1][1].params.cursor).toBe('cursor-1');
    expect(records.map((r) => r.symbol)).toEqual(['BTCUSDT', 'ETHUSDT']);
  });

  it('decomposes one transaction row into a record carrying its symbol', async () => {
    const { exchange, get } = createExchange();
    get.mockResolvedValueOnce(okPage([row('BTCUSDT', '1700000000000')]));

    const records = await exchange.fetchIncomeHistory({});

    // The symbol is what fill-symbol enumeration reads; without it the row is useless.
    expect(records[0]?.symbol).toBe('BTCUSDT');
  });
});

describe('bybit linear income history range', () => {
  // Chunk bounds are INCLUSIVE at both ends, so consecutive chunks must not share
  // an endpoint or the boundary row comes back twice — and it cannot be deduped
  // afterwards, because bybit tranIds are not unique.
  it('advances past each chunk boundary instead of sharing it', async () => {
    const { exchange, get } = createExchange();
    get.mockResolvedValue(okPage([]));

    const sevenDays = 7 * 24 * 60 * 60 * 1000;
    const start = 1_700_000_000_000;
    await exchange.fetchIncomeHistoryRange(start, start + sevenDays * 2 + 5);

    const windows = get.mock.calls.map(([, config]) => [config.params.startTime, config.params.endTime]);
    expect(windows.length).toBeGreaterThan(1);
    for (let i = 1; i < windows.length; i++) {
      expect(windows[i][0]).toBeGreaterThan(windows[i - 1][1]);
    }
  });

  // Deliberately TWO chunks returning rows in ascending order, so dropping the
  // final sort actually fails. A single row cannot prove an ordering.
  it('returns records newest first across chunks', async () => {
    const { exchange, get } = createExchange();
    get.mockResolvedValueOnce(okPage([row('BTCUSDT', '1700000000000')]));
    get.mockResolvedValueOnce(okPage([row('ETHUSDT', '1700600000000')]));
    get.mockResolvedValue(okPage([]));

    const sevenDays = 7 * 24 * 60 * 60 * 1000;
    const start = 1_700_000_000_000;
    const records = await exchange.fetchIncomeHistoryRange(start, start + sevenDays * 2 + 5);

    expect(records.map((r) => r.symbol)).toEqual(['ETHUSDT', 'BTCUSDT']);
  });
  // The cursor is venue-supplied; an endlessly-repeating one must not loop forever.
  it('stops paging at the safety limit even while the venue keeps sending a cursor', async () => {
    const { exchange, get } = createExchange();
    get.mockResolvedValue(okPage([row('BTCUSDT', '1700000000000')], 'never-ending'));

    await exchange.fetchIncomeHistory({});

    expect(get).toHaveBeenCalledTimes(10);
  });

  it('stops paging once the requested record cap is reached', async () => {
    const { exchange, get } = createExchange();
    get.mockResolvedValue(okPage([row('BTCUSDT', '1700000000000'), row('ETHUSDT', '1700000001000')], 'more'));

    const records = await exchange.fetchIncomeHistory({ limit: 3 });

    expect(get).toHaveBeenCalledTimes(2);
    expect(records.length).toBeGreaterThanOrEqual(3);
  });
});

const createInverseExchange = () => {
  const exchange = new BybitInverseExchange(
    'bybitinverse' as any,
    { key: 'key', secret: 'secret', testnet: false } as any,
    createStore(),
  );
  const get = vi.fn();
  exchange.xhr = { get } as any;
  exchange.log = vi.fn();
  return { exchange, get };
};

describe('bybit inverse income history', () => {
  it('asks the transaction log for the INVERSE category', async () => {
    const { exchange, get } = createInverseExchange();
    get.mockResolvedValueOnce(okPage([]));

    await exchange.fetchIncomeHistory({});

    expect(get.mock.calls[0][0]).toBe('/v5/account/transaction-log');
    expect(get.mock.calls[0][1].params.category).toBe('inverse');
  });

  // The transaction log serves UNIFIED accounts only. The class's own
  // `accountType` getter returns CONTRACT for a classic account, and sending that
  // earns a non-zero retCode which this path turns into an empty list — which
  // reads as "you traded nothing". So it is hardcoded, exactly as linear does.
  it('always asks as UNIFIED, even when the account reports CONTRACT', async () => {
    const { exchange, get } = createInverseExchange();
    (exchange as any).unifiedMarginStatus = 1;
    expect(exchange.accountType).toBe('CONTRACT');
    get.mockResolvedValueOnce(okPage([]));

    await exchange.fetchIncomeHistory({});

    expect(get.mock.calls[0][1].params.accountType).toBe('UNIFIED');
  });

  it('carries inverse symbols through, which is the whole point', async () => {
    const { exchange, get } = createInverseExchange();
    get.mockResolvedValueOnce(okPage([row('BTCUSD', '1700000000000')]));

    const records = await exchange.fetchIncomeHistory({});

    expect(records[0]?.symbol).toBe('BTCUSD');
  });
});
