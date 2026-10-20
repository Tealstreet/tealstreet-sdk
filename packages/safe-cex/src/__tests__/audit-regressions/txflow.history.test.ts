import { describe, expect, it, vi } from 'vitest';
import type { TxFlowExchange } from '../reference/txflow/txflow.exchange';
import { fetchHistoricalFills, fetchHistoricalOrders, fetchIncomeHistory, fetchTrades } from '../reference/txflow/txflow.history';
import { mapTxFlowHistoricalIdentities } from '../reference/txflow/txflow.public';
import type { TxFlowInstrument } from '../reference/txflow/txflow.types';
import { DefaultStore } from '../../store/store.base';
import { OrderStatus, type Market } from '../../types';

// Independently authored from the published /api/info history payloads.
const NOW = 1_700_000_000_000;
const WALLET = '0x0000000000000000000000000000000000000001';
const btc: Market = { id: '1', symbol: 'BTC-USDC', base: 'BTC', quote: 'USDC', active: true, precision: { amount: 0.0001, price: 0.1 }, limits: { amount: { min: 0.0001, max: 100 }, leverage: { min: 1, max: 50 } } };
const eth: Market = { ...btc, id: '2', symbol: 'ETH-USDC', base: 'ETH' };
const fill = (overrides: Record<string, unknown> = {}) => ({ instrumentId: 1, tid: 101, oid: 42, time: NOW, px: '65010.5', sz: '0.01', side: 'B', fee: '0.02', feeToken: 'USDC', closedPnl: '3.5', crossed: true, ...overrides });
const order = (overrides: Record<string, unknown> = {}, native: Record<string, unknown> = {}) => ({ order: { instrumentId: 1, oid: 42, timestamp: NOW, side: 'A', orderType: 'Limit', limitPx: '65000', origSz: '2', filledAmount: '0.5', reduceOnly: false, ...native }, status: 'Canceled', statusTimestamp: NOW + 100, ...overrides });
const funding = (overrides: Record<string, unknown> = {}) => ({ coin: 'BTC', time: NOW, amount: '-0.12', szi: '2', fundingRate: '0.01', usdc: '-0.12', ...overrides });
const ledger = (overrides: Record<string, unknown> = {}) => ({ time: NOW, hash: '0xabc', delta: { type: 'deposit', amount: '100', status: 'completed' }, ...overrides });
const page = (rows: unknown[], hasMore = false) => ({ data: rows, hasMore });

function setup(handler: (body: Record<string, unknown>) => unknown | Promise<unknown>) {
  const store = new DefaultStore();
  store.update({ markets: [btc, eth] });
  const request = vi.fn(async (body: Record<string, unknown>) => {
    expect(body.user).toBe(WALLET);
    return handler(body);
  });
  const exchange = {
    store,
    options: { testnet: true, applicationId: WALLET },
    isDisposed: false,
    info: <T>(body: Record<string, unknown>) => request(body) as Promise<T>,
  } as unknown as TxFlowExchange;
  return { exchange, request };
}

describe('TxFlow indexed history correctness', () => {
  it('applies symbol, order and exact millisecond bounds before converting executions to seconds', async () => {
    const { exchange, request } = setup(() => page([
      fill({ tid: 104, time: NOW + 2 }),
      fill({ tid: 103, time: NOW + 1 }),
      fill({ tid: 102, time: NOW }),
      fill({ tid: 105, instrumentId: 2, time: NOW + 1 }),
      fill({ tid: 106, oid: 43, time: NOW + 1 }),
      fill({ tid: 103, time: NOW + 1 }),
    ]));
    const rows = await fetchHistoricalFills(exchange, { symbol: btc.symbol, orderId: '42', from: NOW + 1, to: NOW + 1 });
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ id: '103', orderId: '42', timestamp: (NOW + 1) / 1000, symbol: btc.symbol, price: 65010.5, qty: 0.01, commission: 0.02, realizedPnl: 3.5, maker: false });
    expect(request).toHaveBeenCalledWith(expect.objectContaining({ type: 'userFills', coin: '1', startTimeMs: NOW + 1, endTimeMs: NOW + 1 }));
  });

  it('uses hasMore on a short raw page and applies output limits after collecting later records', async () => {
    const { exchange, request } = setup((body) => body.offset === 0 ? page([fill({ tid: 1 })], true) : page([fill({ tid: 2, time: NOW + 1 })]));
    expect((await fetchHistoricalFills(exchange, { limit: 1 }))[0].id).toBe('2');
    expect(request.mock.calls.map(([body]) => body.offset)).toEqual([0, 2000]);
  });

  it('rejects failed continuation reads instead of returning already collected rows', async () => {
    const { exchange } = setup((body) => {
      if (body.offset === 0) return page([fill()], true);
      throw new Error('upstream unavailable');
    });
    await expect(fetchHistoricalFills(exchange, {})).rejects.toThrow('upstream unavailable');
  });

  it('rejects malformed pages and empty continuations explicitly', async () => {
    for (const response of [{ data: [], hasMore: 'false' }, { hasMore: false }, page([], true)]) {
      const { exchange } = setup(() => response);
      await expect(fetchHistoricalFills(exchange, {})).rejects.toThrow(/Malformed|Incomplete/);
    }
  });

  it('fails a repeated non-progressing page within the documented offset cap', async () => {
    const { exchange, request } = setup(() => page([fill()], true));
    await expect(fetchHistoricalFills(exchange, { limit: 1 })).rejects.toThrow('Incomplete TxFlow history');
    expect(request.mock.calls.length).toBeLessThanOrEqual(101);
  });

  it('keeps local order range and symbol filters, and preserves total versus filled quantities', async () => {
    const { exchange } = setup(() => page([
      order(),
      order({}, { oid: 43, instrumentId: 2 }),
      order({}, { oid: 44, timestamp: NOW - 1 }),
      order({}, { oid: 45, timestamp: NOW + 1 }),
    ]));
    const rows = await fetchHistoricalOrders(exchange, { symbol: btc.symbol, from: NOW, to: NOW });
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ id: '42', symbol: btc.symbol, amount: 2, filled: 0.5, status: OrderStatus.Canceled, createdTime: NOW, updatedTime: NOW + 100 });
  });

  it('rejects unknown order status instead of inventing a cancellation', async () => {
    const { exchange } = setup(() => page([order({ status: 'NativeStatusAddedTomorrow' })]));
    await expect(fetchHistoricalOrders(exchange)).rejects.toThrow('Unknown TxFlow history order status');
  });

  it('keeps opposite funding legs at the same coin and timestamp distinct', async () => {
    const { exchange } = setup(() => page([
      funding(),
      funding({ amount: '0.12', szi: '-2', usdc: '0.12' }),
    ]));
    const rows = await fetchIncomeHistory(exchange, { incomeType: 'FUNDING_FEE' });
    expect(rows).toHaveLength(2);
    expect(rows.map((row) => row.income).sort()).toEqual([-0.12, 0.12]);
    expect(new Set(rows.map((row) => row.tranId)).size).toBe(2);
  });

  it('fails ambiguous identical funding records rather than silently collapsing a leg', async () => {
    const { exchange } = setup(() => page([funding(), funding()]));
    await expect(fetchIncomeHistory(exchange, { incomeType: 'FUNDING_FEE' })).rejects.toThrow('Ambiguous TxFlow funding identity');
  });

  it('applies local funding symbol and millisecond bounds even if the upstream ignores them', async () => {
    const { exchange } = setup(() => page([
      funding(),
      funding({ coin: 'ETH' }),
      funding({ time: NOW - 1 }),
      funding({ time: NOW + 1 }),
    ]));
    const rows = await fetchIncomeHistory(exchange, { incomeType: 'FUNDING_FEE', symbol: btc.symbol, startTime: NOW, endTime: NOW });
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ symbol: btc.symbol, income: -0.12, timestamp: NOW });
  });

  it('requires nonempty ledger identity and filters account-wide transfers out of symbol queries', async () => {
    const { exchange, request } = setup(() => page([ledger({ hash: '' })]));
    await expect(fetchIncomeHistory(exchange, { incomeType: 'deposit' })).rejects.toThrow('Missing TxFlow ledger identity');
    request.mockClear();
    await expect(fetchIncomeHistory(exchange, { incomeType: 'deposit', symbol: btc.symbol })).resolves.toEqual([]);
    expect(request).not.toHaveBeenCalled();
  });

  it('bounds native ledger transfers locally and keeps withdrawals signed negative', async () => {
    const { exchange, request } = setup(() => page([
      ledger({ time: NOW - 1, hash: '0xold' }),
      ledger(),
      ledger({ hash: '0xwithdraw', delta: { type: 'withdraw', amount: '30', status: 'completed' } }),
      ledger({ time: NOW + 1, hash: '0xfuture' }),
    ]));
    const deposits = await fetchIncomeHistory(exchange, { incomeType: 'deposit', startTime: NOW, endTime: NOW });
    expect(deposits).toHaveLength(1);
    expect(deposits[0]).toMatchObject({ symbol: '', income: 100, timestamp: NOW, tranId: 'ledger:0xabc:deposit' });
    const withdrawals = await fetchIncomeHistory(exchange, { incomeType: 'withdraw', startTime: NOW, endTime: NOW });
    expect(withdrawals).toHaveLength(1);
    expect(withdrawals[0]).toMatchObject({ symbol: '', income: -30 });
    expect(request.mock.calls[0][0]).toMatchObject({ deltaTypes: ['deposit', 'withdraw'] });
    expect(request.mock.calls[0][0]).not.toHaveProperty('accountType');
  });

  it('keeps native realized PnL separate from paid fees and maker rebates', async () => {
    const { exchange } = setup(() => page([fill(), fill({ tid: 102, fee: '-0.01', closedPnl: '-2' })]));
    const pnl = await fetchIncomeHistory(exchange, { incomeType: 'REALIZED_PNL' });
    const fees = await fetchIncomeHistory(exchange, { incomeType: 'COMMISSION' });
    expect(pnl.map((row) => row.income).sort()).toEqual([-2, 3.5]);
    expect(fees.map((row) => row.income).sort()).toEqual([-0.02, 0.01]);
    expect(new Set([...pnl, ...fees].map((row) => row.tranId)).size).toBe(4);
  });

  it('fails unsafe native IDs and invalid query ranges before reporting history', async () => {
    const { exchange, request } = setup(() => page([fill({ tid: Number.MAX_SAFE_INTEGER + 1 })]));
    await expect(fetchHistoricalFills(exchange, {})).rejects.toThrow('Unsafe TxFlow native history ID');
    request.mockClear();
    await expect(fetchHistoricalFills(exchange, { from: NOW + 1, to: NOW })).rejects.toThrow('Invalid TxFlow history range');
    expect(request).not.toHaveBeenCalled();
    expect(() => fetchTrades()).toThrow('does not document a REST public trades history query');
  });
});

it('resolves retired instruments for account-wide fills, orders, PnL and funding without active markets', async () => {
  const { exchange, request } = setup((body) => body.type === 'historicalOrders' ? page([order({}, { instrumentId: 9 })]) : body.type === 'userFundings' ? page([funding({ coin: 'OLD' })]) : page([fill({ instrumentId: 9 })]));
  const retired: TxFlowInstrument = { index: 9, name: 'OLD-USDC', baseCurrency: 'OLD', quoteCurrency: 'USDC', currencyType: 'usdc', delisted: true, haltTrading: true, basePrecision: '0', priceTick: '0', maxLeverage: 0, onlyIsolated: false, useNotionalMargin: false };
  exchange.historicalIdentities = mapTxFlowHistoricalIdentities([retired]);
  expect(exchange.store.markets.some((m) => m.id === '9')).toBe(false);
  expect((await fetchHistoricalFills(exchange, {}))[0].symbol).toBe('OLD-USDC');
  expect((await fetchHistoricalOrders(exchange, { symbol: 'OLD-USDC' }))[0].symbol).toBe('OLD-USDC');
  expect((await fetchIncomeHistory(exchange, { symbol: 'OLD-USDC', incomeType: 'REALIZED_PNL' }))[0].symbol).toBe('OLD-USDC');
  expect((await fetchIncomeHistory(exchange, { symbol: 'OLD-USDC', incomeType: 'FUNDING_FEE' }))[0].symbol).toBe('OLD-USDC');
  expect(request.mock.calls.filter(([b]) => b.type !== 'userFills' || b.coin).every(([b]) => b.coin === '9')).toBe(true);
});
it('retains relisted native IDs but rejects an ambiguous historical symbol filter', async () => {
  const { exchange } = setup(() => page([fill({ instrumentId: 9 })]));
  const active: TxFlowInstrument = { index: 1, name: 'BTC-USDC', baseCurrency: 'BTC', quoteCurrency: 'USDC', currencyType: 'usdc', delisted: false, haltTrading: false, basePrecision: '0.0001', priceTick: '0.1', maxLeverage: 50, onlyIsolated: false, useNotionalMargin: false };
  exchange.historicalIdentities = mapTxFlowHistoricalIdentities([active, { ...active, index: 9, delisted: true }]);
  expect((await fetchHistoricalFills(exchange, {}))[0].symbol).toBe('BTC-USDC');
  await expect(fetchHistoricalFills(exchange, { symbol: 'BTC-USDC' })).rejects.toThrow('ambiguous');
});
it('continues to reject an unknown historical ID instead of fabricating an SDK symbol', async () => {
  const { exchange } = setup(() => page([fill({ instrumentId: 999 })]));
  await expect(fetchHistoricalFills(exchange, {})).rejects.toThrow('Unknown TxFlow history instrument');
});
