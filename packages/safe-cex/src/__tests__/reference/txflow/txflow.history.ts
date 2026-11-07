import type { Execution, ExecutionsOptions, HistoricalOrder, IncomeHistoryOptions, IncomeRecord, OrderHistoryOptions } from '../../../types';
import { OrderSide, OrderStatus, OrderType } from '../../../types';
import type { TxFlowExchange } from './txflow.exchange';

type Page<T> = { data: T[]; hasMore: boolean };
type Fill = {
  instrumentId: number; tid: number; oid: number; time: number; px: string; sz: string;
  side: string; fee: string; feeToken: string; closedPnl: string; crossed: boolean;
};
type NativeOrder = {
  order: { instrumentId: number; oid: number; timestamp: number; side: string; orderType: string;
    limitPx: string; origSz: string; filledAmount: string; reduceOnly: boolean };
  status: string; statusTimestamp: number;
};
type Funding = { time: number; coin: string; amount: string; szi: string; fundingRate: string; usdc: string };
type Ledger = { time: number; hash: string; delta: { type: string; amount: string; status?: string; token?: string; user?: string; destination?: string } };

const number = (value: unknown): number => {
  if ((typeof value !== 'string' && typeof value !== 'number') || value === '') throw new Error('Malformed TxFlow history number');
  const n = Number(value);
  if (!Number.isFinite(n)) throw new Error('Malformed TxFlow history number');
  return n;
};
const id = (value: unknown): string => {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) throw new Error('Unsafe TxFlow native history ID');
  return String(value);
};
const side = (value: string): OrderSide => {
  if (value === 'B') return OrderSide.Buy;
  if (value === 'A') return OrderSide.Sell;
  throw new Error('Malformed TxFlow history side');
};
const symbolFor = (exchange: TxFlowExchange, asset: number | string): string => {
  const market = exchange.historicalIdentities?.get(String(asset).toLowerCase()) ?? exchange.store.markets.find((m) => m.id === String(asset) || m.symbol === asset || m.base === asset);
  if (!market) throw new Error('Unknown TxFlow history instrument');
  return market.symbol;
};
const coinFor = (exchange: TxFlowExchange, symbol?: string) => {
  if (!symbol) return {};
  const market = exchange.historicalIdentities?.get(symbol.toLowerCase()) ?? exchange.store.markets.find((m) => m.symbol === symbol);
  if (!market || ('ambiguous' in market && market.ambiguous)) throw new Error('Unknown or ambiguous TxFlow history symbol');
  return { coin: market.id };
};
const range = (from?: number, to?: number) => {
  if ([from, to].some((n) => n !== undefined && (!Number.isSafeInteger(n) || n < 0)) || (from !== undefined && to !== undefined && from > to))
    throw new Error('Invalid TxFlow history range');
  return { ...(from === undefined ? {} : { startTimeMs: from }), ...(to === undefined ? {} : { endTimeMs: to }) };
};
const limitOf = (limit?: number) => {
  if (limit !== undefined && (!Number.isSafeInteger(limit) || limit <= 0)) throw new Error('Invalid TxFlow history limit');
  return limit ?? Infinity;
};

async function pages<T>(exchange: TxFlowExchange, type: string, filters: Record<string, unknown>): Promise<T[]> {
  const user = exchange.options.vault || exchange.options.applicationId;
  if (!user || !exchange.options.testnet) throw new Error('TxFlow testnet account identity required');
  const result: T[] = [];
  for (let offset = 0; offset <= 200000; offset += 2000) {
    if (exchange.isDisposed) throw new Error('TxFlow disposed during history read');
    const page = await exchange.info<Page<T>>({ type, user, ...(type === 'userNonFundingLedgerUpdates' ? {} : { accountType: ['perp'] }), ...filters, offset, limit: 2000 });
    if (!page || !Array.isArray(page.data) || typeof page.hasMore !== 'boolean') throw new Error('Malformed TxFlow history page');
    if (page.hasMore && page.data.length === 0) throw new Error('Incomplete TxFlow history: empty continuation');
    result.push(...page.data);
    if (!page.hasMore) return result;
  }
  throw new Error('Incomplete TxFlow history: offset cap reached');
}

export async function fetchHistoricalFills(exchange: TxFlowExchange, opts: ExecutionsOptions): Promise<Execution[]> {
  const maximum = limitOf(opts.limit);
  const rows = await pages<Fill>(exchange, 'userFills', { ...coinFor(exchange, opts.symbol), ...range(opts.from, opts.to) });
  const seen = new Set<string>();
  const result: Execution[] = [];
  rows.sort((a, b) => b.time - a.time || b.tid - a.tid);
  for (const row of rows) {
    const nativeId = id(row.tid);
    if (seen.has(nativeId)) continue;
    seen.add(nativeId);
    const orderId = id(row.oid);
    const symbol = symbolFor(exchange, row.instrumentId);
    if (opts.symbol !== undefined && symbol !== opts.symbol) continue;
    if (opts.orderId !== undefined && opts.orderId !== orderId) continue;
    const timestamp = number(row.time);
    if ((opts.from !== undefined && timestamp < opts.from) || (opts.to !== undefined && timestamp > opts.to)) continue;
    const price = number(row.px), qty = number(row.sz);
    if (price <= 0 || qty <= 0 || typeof row.crossed !== 'boolean') throw new Error('Malformed TxFlow execution');
    result.push({ id: nativeId, orderId, timestamp: timestamp / 1000, symbol,
      price, qty, side: side(row.side), commission: number(row.fee), commissionAsset: row.feeToken,
      realizedPnl: number(row.closedPnl), maker: !row.crossed });
  }
  return result.slice(0, maximum);
}

export async function fetchHistoricalOrders(exchange: TxFlowExchange, opts: OrderHistoryOptions = {}): Promise<HistoricalOrder[]> {
  const maximum = limitOf(opts.limit);
  const filters = { ...coinFor(exchange, opts.symbol), ...range(opts.from, opts.to) };
  const rows = await pages<NativeOrder>(exchange, 'historicalOrders', filters);
  const result = new Map<string, HistoricalOrder>();
  for (const row of rows) {
    const o = row.order;
    const orderId = id(o.oid);
    if (opts.orderId !== undefined && opts.orderId !== orderId) continue;
    const state = row.status.toLowerCase();
    if (!['filled', 'open', 'pending', 'partiallyfilled', 'partialfilled', 'triggered', 'canceled', 'cancelled', 'partialcanceled', 'partiallycanceled', 'rejected'].includes(state)) throw new Error('Unknown TxFlow history order status');
    const status = state === 'filled' ? OrderStatus.Closed : ['open', 'pending', 'partiallyfilled', 'partialfilled', 'triggered'].includes(state) ? OrderStatus.Open : OrderStatus.Canceled;
    const time = number(o.timestamp);
    const symbol = symbolFor(exchange, o.instrumentId);
    if ((opts.from !== undefined && time < opts.from) || (opts.to !== undefined && time > opts.to) || (opts.symbol !== undefined && symbol !== opts.symbol)) continue;
    const order: HistoricalOrder = { id: orderId, symbol, side: side(o.side),
      type: o.orderType === 'Market' ? OrderType.Market : OrderType.Limit, status,
      price: number(o.limitPx), amount: number(o.origSz), filled: number(o.filledAmount), reduceOnly: o.reduceOnly,
      createdTime: number(o.timestamp), updatedTime: number(row.statusTimestamp) };
    if (opts.status && !opts.status.includes(status)) continue;
    const prior = result.get(orderId);
    if (!prior || (prior.updatedTime ?? 0) < (order.updatedTime ?? 0)) result.set(orderId, order);
  }
  return [...result.values()].sort((a, b) => b.createdTime - a.createdTime || a.id.localeCompare(b.id)).slice(0, maximum);
}

export async function fetchIncomeHistory(exchange: TxFlowExchange, opts: IncomeHistoryOptions = {}): Promise<IncomeRecord[]> {
  const maximum = limitOf(opts.limit);
  const filters = { ...coinFor(exchange, opts.symbol), ...range(opts.startTime, opts.endTime) };
  const records: IncomeRecord[] = [];
  const include = (kind: string) => opts.incomeType === undefined || opts.incomeType === kind;
  if (include('REALIZED_PNL') || include('COMMISSION')) {
    const fills = await fetchHistoricalFills(exchange, { symbol: opts.symbol, from: opts.startTime, to: opts.endTime });
    for (const row of fills) {
      if (include('REALIZED_PNL')) records.push({ symbol: row.symbol, incomeType: 'REALIZED_PNL', income: row.realizedPnl!, asset: 'USDC', timestamp: row.timestamp * 1000, tranId: `pnl:${row.id}`, tradeId: row.id, category: 'realized_pnl' });
      if (include('COMMISSION')) records.push({ symbol: row.symbol, incomeType: 'COMMISSION', income: -row.commission!, asset: row.commissionAsset!, timestamp: row.timestamp * 1000, tranId: `fee:${row.id}`, tradeId: row.id, category: 'commission' });
    }
  }
  if (include('FUNDING_FEE')) {
    const rows = await pages<Funding>(exchange, 'userFundings', filters);
    const identities = new Set<string>();
    for (const row of rows) {
      const symbol = symbolFor(exchange, row.coin), timestamp = number(row.time);
      const tranId = `funding:${row.coin}:${row.time}:${row.szi}:${row.amount}:${row.fundingRate}:${row.usdc}`;
      number(row.szi); number(row.fundingRate); number(row.usdc);
      if (identities.has(tranId)) throw new Error('Ambiguous TxFlow funding identity; range may be incomplete');
      identities.add(tranId);
      if ((opts.symbol !== undefined && symbol !== opts.symbol) || (opts.startTime !== undefined && timestamp < opts.startTime) || (opts.endTime !== undefined && timestamp > opts.endTime)) continue;
      records.push({ symbol, incomeType: 'FUNDING_FEE', income: number(row.amount), asset: 'USDC', timestamp, tranId, category: 'funding' });
    }
  }
  if (opts.symbol === undefined && (include('deposit') || include('withdraw'))) {
    const rows = await pages<Ledger>(exchange, 'userNonFundingLedgerUpdates', { ...range(opts.startTime, opts.endTime), deltaTypes: ['deposit', 'withdraw'] });
    for (const row of rows) {
      if (typeof row.hash !== 'string' || row.hash.length === 0) throw new Error('Missing TxFlow ledger identity');
      if ((opts.startTime !== undefined && row.time < opts.startTime) || (opts.endTime !== undefined && row.time > opts.endTime)) continue;
      if (!include(row.delta.type) || !['deposit', 'withdraw'].includes(row.delta.type)) continue;
      if (row.delta.status !== 'completed') throw new Error('Unverified TxFlow ledger settlement status');
      records.push({ symbol: '', incomeType: row.delta.type, income: number(row.delta.amount) * (row.delta.type === 'withdraw' ? -1 : 1), asset: 'USDC', timestamp: number(row.time), tranId: `ledger:${row.hash}:${row.delta.type}`, category: 'transfer' });
    }
  }
  return [...new Map(records.map((r) => [r.tranId, r])).values()].sort((a, b) => b.timestamp - a.timestamp || a.tranId.localeCompare(b.tranId)).slice(0, maximum);
}

export const fetchIncomeHistoryRange = (exchange: TxFlowExchange, startTime: number, endTime: number, incomeType?: string) =>
  fetchIncomeHistory(exchange, { startTime, endTime, incomeType });

export function fetchTrades(): never {
  throw new Error('TxFlow does not document a REST public trades history query; use the trades stream');
}
