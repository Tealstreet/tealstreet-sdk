import { afterEach, describe, expect, it, vi } from 'vitest';

import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { decodeParadexMessage } from '../reference/paradex/paradex.sbe';
import { DefaultStore } from '../../store/store.base';
import { OrderStatus } from '../../types';
import fixture from '../fixtures/paradex-sbe.json';

const bytes = (base64: string) => Uint8Array.from(Buffer.from(base64, 'base64'));
const decode = (base64: string) => decodeParadexMessage(bytes(base64))!.params.data;

describe('Paradex SBE schema 1:2 against the official Python codec', () => {
  it.each(fixture.cases)('$name preserves numeric, identity and status fields', ({ name, base64, expected }) => {
    const row = decode(base64);
    const oracle = expected as Record<string, any>;
    const fields: Record<string, string> = { market: 'market', timestamp: 'timestamp' };
    if (name.startsWith('summary')) delete fields.market;
    if (name.startsWith('trade')) Object.assign(fields, { price: 'price', size: 'size', created_at: 'created_at', side: 'side' });
    else if (name.startsWith('bbo')) Object.assign(fields, { bid_price: 'bid_price', ask_price: 'ask_price', bid_size: 'bid_size', ask_size: 'ask_size' });
    else if (name.startsWith('summary')) Object.assign(fields, {
      symbol: 'market', mark_price: 'mark_price', underlying_price: 'index_price', last_traded_price: 'last_price',
      volume_24h: 'volume_usd24h', open_interest: 'open_interest', bid: 'bid', ask: 'ask', price_change_rate_24h: 'price_change_rate24h',
    });
    else if (name.startsWith('order')) Object.assign(fields, {
      id: 'order_id', client_id: 'client_order_id', type: 'order_type', status: 'status', side: 'side',
      price: 'price', trigger_price: 'trigger_price', size: 'size', remaining_size: 'size_open',
      avg_fill_price: 'avg_fill_price', created_at: 'created_at', last_updated_at: 'updated_at',
      instruction: 'time_in_force', flags: 'flags', request_info: 'request_info',
    });
    else if (name.startsWith('fill')) Object.assign(fields, {
      id: 'fill_id', order_id: 'order_id', client_id: 'client_order_id', trade_id: 'trade_id', side: 'side',
      price: 'price', size: 'size', fee: 'fee', fee_currency: 'fee_currency', realized_pnl: 'realized_pnl',
      created_at: 'created_at', underlying_price: 'underlying_price', realized_funding: 'realized_funding', liquidity: 'liquidity',
    });
    else if (name.startsWith('position')) Object.assign(fields, {
      size: 'size', side: 'side', average_entry_price: 'avg_entry_price', unrealized_pnl: 'unrealized_pnl',
      realized_positional_pnl: 'realized_pnl', mark_price: 'mark_price', liquidation_price: 'liquidation_price',
      leverage: 'leverage', last_updated_at: 'updated_at', cost: 'cost', cost_usd: 'cost_usd',
      cached_funding_index: 'cached_funding_index', unrealized_funding_pnl: 'unrealized_funding_pnl',
      status: 'status', last_fill_id: 'last_fill_id',
    });
    else if (name.startsWith('account')) Object.assign(fields, {
      total_collateral: 'total_collateral', free_collateral: 'free_collateral', initial_margin_requirement: 'initial_margin_req',
      maintenance_margin_requirement: 'maintenance_margin_req', account_value: 'account_value', unrealized_pnl: 'unrealized_pnl',
      updated_at: 'updated_at', margin_cushion: 'margin_cushion', status: 'status', settlement_asset: 'settlement_asset',
      last_seen_notification: 'last_seen_notification',
    });
    for (const [field, source] of Object.entries(fields)) {
      if (source in oracle) expect(row[field], `${name}.${field}`).toEqual(oracle[source] ?? undefined);
    }
    expect(row.seq_no).toBe(String(oracle.seq_no));
    if (name.startsWith('trade')) expect(row.id).toBe(oracle.trade_id_str || String(oracle.trade_id));
    if (name.startsWith('summary')) expect(row.funding_rate).toBe(oracle.funding_rate_precise ?? oracle.funding_rate);
    if (name.startsWith('book')) {
      expect(row.update_type).toBe(oracle.pkg_type === 'SNAPSHOT' ? 's' : 'd');
      const levels = [...oracle.bids.map(([price, size]: string[]) => ({ side: 'BUY', price, size })),
        ...oracle.asks.map(([price, size]: string[]) => ({ side: 'SELL', price, size }))];
      expect(row[row.update_type === 's' ? 'inserts' : 'updates']).toEqual(levels);
      expect(row.deletes).toEqual([]);
    }
  });

  it.each(fixture.cases.filter(({ name }) => ['bbo', 'book-snapshot', 'position-long'].includes(name)))(
    '$name rejects every truncated prefix', ({ base64 }) => {
      const data = bytes(base64);
      for (let length = 0; length < data.length; length++) expect(() => decodeParadexMessage(data.subarray(0, length)), `length=${length}`).toThrow();
    },
  );

  it('honors typed-array byte offsets and rejects unsupported schemas and group dimensions', () => {
    const data = bytes(fixture.cases.find(({ name }) => name === 'book-snapshot')!.base64);
    const backing = new Uint8Array(data.length + 10);
    backing.set(data, 5);
    expect(decodeParadexMessage(backing.subarray(5, 5 + data.length))).toEqual(decodeParadexMessage(data));
    for (const [offset, value] of [[4, 2], [6, 3], [97, 15], [99, 255], [100, 255]]) {
      const malformed = data.slice(); malformed[offset] = value;
      expect(() => decodeParadexMessage(malformed)).toThrow();
    }
  });

  it.each([
    ['account-v2', 16], ['account-v2', 24], ['position-long', 17], ['order-v2', 36], ['order-v2', 44], ['fill-v2', 27],
  ] as const)('%s rejects a null mandatory scalar at offset %s', (name, offset) => {
    const data = bytes(fixture.cases.find((row) => row.name === name)!.base64);
    new DataView(data.buffer).setBigInt64(8 + offset, -(1n << 63n), true);
    expect(() => decodeParadexMessage(data)).toThrow('Null mandatory');
  });
});

afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });

it('handles CLOSED SBE orders as final and prevents an older snapshot reopening them', () => {
  vi.useFakeTimers();
  const exchange = new ParadexExchange('paradex', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  const data = bytes(fixture.cases.find(({ name }) => name === 'order-v2')!.base64);
  data[24] = 4;
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  exchange.privateWebsocket.onMessage({ data } as MessageEvent);
  const current = exchange.store.orders[0];
  expect(current).toMatchObject({ id: 'order-123', status: OrderStatus.Canceled, remaining: 0.5 });
  exchange.store.update({ orders: [{ ...current, status: OrderStatus.Open }] });
  expect(exchange.store.orders[0].status).toBe(OrderStatus.Canceled);
  exchange.dispose();
});

it.each(['bbo', 'summary'])('applies every %s update across different markets in the same throttle window', (name) => {
  vi.useFakeTimers();
  const exchange = new ParadexExchange('paradex', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  const symbols = ['BTC-USD-PERP', 'ETH-USD-PERP'];
  exchange.store.update({ tickers: symbols.map((symbol) => ({ id: symbol, symbol, bid: 0, ask: 0, last: 0,
    mark: 0, index: 0, percentage: 0, volume: 0, quoteVolume: 0, openInterest: 0, fundingRate: 0,
    fundingTime: 0, timestamp: 0, info: {} })) });
  const template = bytes(fixture.cases.find((row) => row.name === name)!.base64);
  const send = (symbol: string, price: number) => {
    const data = template.slice();
    const view = new DataView(data.buffer);
    const market = 8 + view.getUint16(0, true) + 1;
    data.set(new TextEncoder().encode(symbol), market);
    view.setBigInt64(24, BigInt(price) * 100000000n, true);
    exchange.publicWebsocket.onMessage({ data } as MessageEvent);
  };
  send(symbols[0], 42000); send(symbols[1], 5000); send(symbols[0], 42001); send(symbols[1], 5001);
  const field = name === 'bbo' ? 'bid' : 'mark';
  expect(exchange.store.tickers.map((row) => row[field])).toEqual([42001, 5001]);
  exchange.dispose();
});
