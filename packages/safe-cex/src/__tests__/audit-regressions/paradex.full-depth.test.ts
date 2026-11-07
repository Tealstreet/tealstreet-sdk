import type { OrderBook } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { DefaultStore } from '../../store/store.base';

// Official ResponsesPriceBookUpdate and order_book deltas feed contract:
// https://github.com/tradeparadex/paradex-docs/blob/main/fern/apis/prod_ws/asyncapi-2.6.0.yaml
const BTC = 'BTC-USD-PERP';
const ETH = 'ETH-USD-PERP';
const level = (side: 'BUY' | 'SELL', price: number, size: number) => ({ side, price: String(price), size: String(size) });

describe('Paradex full-depth order books', () => {
  const exchanges: ParadexExchange[] = [];
  const setup = () => {
    const exchange = new ParadexExchange('paradex', { key: '', secret: '', publicDataOnly: true, tuning: { consoleLogging: false } }, new DefaultStore());
    exchanges.push(exchange);
    const ws = exchange.publicWebsocket;
    Object.defineProperty(ws, 'isConnected', { configurable: true, get: () => true });
    const send = vi.spyOn(ws as any, 'send').mockImplementation(() => {});
    const books = new Map<string, OrderBook[]>();
    const listen = (symbol = BTC) => {
      const rows: OrderBook[] = [];
      books.set(symbol, rows);
      return ws.listenOrderBook(symbol, (book) => rows.push(book));
    };
    const update = (patch: Record<string, unknown> = {}, symbol = BTC) => ws.onMessage({
      data: JSON.stringify({ jsonrpc: '2.0', method: 'subscription', params: {
        channel: `order_book.${symbol}.deltas`, data: {
          market: symbol, seq_no: 100, update_type: 's',
          inserts: [level('BUY', 99, 1), level('SELL', 101, 2)], updates: [], deletes: [], ...patch,
        },
      } }),
    } as MessageEvent);
    return { ws, send, books, listen, update };
  };

  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    exchanges.splice(0).forEach((exchange) => exchange.dispose());
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('subscribes to the full-depth feed and preserves more than fifteen levels in sorted order', () => {
    const { send, listen, books, update } = setup();
    listen();
    expect(JSON.parse(send.mock.calls[0][0] as string)).toMatchObject({ method: 'subscribe', params: { channel: `order_book.${BTC}.deltas` } });
    update({ inserts: [...Array.from({ length: 30 }, (_, i) => level('BUY', 70 + i, 1)), level('SELL', 101, 2)] });
    expect(books.get(BTC)?.[0].bids).toHaveLength(30);
    expect(books.get(BTC)?.[0].bids[0].price).toBe(99);
    expect(books.get(BTC)?.[0].bids[29].price).toBe(70);
  });

  it('applies every incremental frame before throttled emission, including deletes and zero sizes', () => {
    const { listen, books, update } = setup();
    listen();
    update();
    update({ seq_no: 101, update_type: 'd', inserts: [level('BUY', 98, 3)], updates: [level('SELL', 101, 4)] });
    update({ seq_no: 102, update_type: 'd', inserts: [level('SELL', 102, 5)], updates: [], deletes: [level('BUY', 99, 0)] });
    vi.advanceTimersByTime(100);
    expect(books.get(BTC)?.at(-1)).toEqual({
      bids: [{ price: 98, amount: 3, total: 294 }],
      asks: [{ price: 101, amount: 4, total: 404 }, { price: 102, amount: 5, total: 510 }],
    });
    update({ seq_no: 103, update_type: 'd', inserts: [], updates: [level('SELL', 101, 0)], deletes: [] });
    vi.advanceTimersByTime(100);
    expect(books.get(BTC)?.at(-1)?.asks).toEqual([{ price: 102, amount: 5, total: 510 }]);
  });

  it('ignores duplicate and older frames without rewinding the book', () => {
    const { listen, books, update } = setup();
    listen(); update();
    update({ seq_no: 100, update_type: 'd', inserts: [level('BUY', 100, 9)] });
    update({ seq_no: 99, inserts: [level('BUY', 90, 9)] });
    vi.advanceTimersByTime(100);
    expect(books.get(BTC)).toHaveLength(1);
    expect(books.get(BTC)?.[0].bids[0].price).toBe(99);
  });

  it('resubscribes on a gap and refuses further deltas until a fresh snapshot', () => {
    const { send, listen, books, update } = setup();
    listen(); update(); send.mockClear();
    update({ seq_no: 102, update_type: 'd', inserts: [level('BUY', 100, 1)] });
    update({ seq_no: 103, update_type: 'd', inserts: [level('BUY', 100, 2)] });
    vi.advanceTimersByTime(100);
    expect(books.get(BTC)).toHaveLength(1);
    expect(send.mock.calls.map(([text]) => JSON.parse(text as string).method)).toEqual(['unsubscribe', 'subscribe']);
    update({ seq_no: 104, inserts: [level('BUY', 90, 4), level('SELL', 110, 5)] });
    expect(books.get(BTC)?.at(-1)?.bids).toEqual([{ price: 90, amount: 4, total: 360 }]);
  });

  it.each([
    { seq_no: undefined }, { inserts: [{ side: 'BUY', price: '99', size: 'NaN' }] },
    { updates: null }, { update_type: undefined },
  ])('rejects malformed frames atomically and recovers: %j', (patch) => {
    const { send, listen, books, update } = setup();
    listen(); update(); send.mockClear();
    update({ seq_no: 101, update_type: 'd', ...patch });
    vi.advanceTimersByTime(100);
    expect(books.get(BTC)).toHaveLength(1);
    expect(send).toHaveBeenCalledTimes(2);
    update();
    expect(books.get(BTC)).toHaveLength(2);
  });

  it('keeps independent emission timers for simultaneous symbols', () => {
    const { listen, books, update } = setup();
    listen(BTC); listen(ETH);
    update({}, BTC); update({}, ETH);
    expect(books.get(BTC)).toHaveLength(1);
    expect(books.get(ETH)).toHaveLength(1);
  });

  it('resets snapshot state when the connection reopens', () => {
    const { ws, listen, books, update } = setup();
    listen(); update();
    ws.onOpen();
    update({ seq_no: 101, update_type: 'd' });
    expect(books.get(BTC)).toHaveLength(1);
    update({ seq_no: 1 });
    expect(books.get(BTC)).toHaveLength(2);
  });

  it('cancels pending emissions and drops state when the last listener leaves', () => {
    const { listen, books, update } = setup();
    const stop = listen(); update();
    update({ seq_no: 101, update_type: 'd', inserts: [level('BUY', 98, 3)] });
    stop();
    vi.advanceTimersByTime(1000);
    expect(books.get(BTC)).toHaveLength(1);
  });

  it('compares large sequence numbers without floating-point rounding', () => {
    const { ws, listen, books } = setup();
    listen();
    const send = (sequence: string, kind: string) => ws.onMessage({ data:
      `{"jsonrpc":"2.0","method":"subscription","params":{"channel":"order_book.${BTC}.deltas","data":{"market":"${BTC}","seq_no":${sequence},"update_type":"${kind}","inserts":[{"side":"BUY","price":"99","size":"1"}],"updates":[],"deletes":[]}}}`,
    } as MessageEvent);
    send('9007199254740992', 's'); send('9007199254740993', 'd');
    vi.advanceTimersByTime(100);
    expect(books.get(BTC)).toHaveLength(2);
  });

  it.each([undefined, null, { channel: 'bbo.ETH-USD-PERP' }])('retries malformed or mismatched subscription acknowledgements: %j', (result) => {
    const { ws, listen, send } = setup();
    listen();
    const request = JSON.parse(send.mock.calls[0][0] as string);
    ws.onMessage({ data: JSON.stringify({ jsonrpc: '2.0', id: request.id, result }) } as MessageEvent);
    vi.advanceTimersByTime(1000);
    expect(send).toHaveBeenCalledTimes(2);
    expect(JSON.parse(send.mock.calls[1][0] as string).params.channel).toBe(request.params.channel);
  });
});
