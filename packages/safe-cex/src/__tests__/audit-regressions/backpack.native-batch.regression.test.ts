import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { collectAuthParams, getInstruction } from '../reference/backpack/backpack.api';
import { BackpackExchange } from '../reference/backpack/backpack.exchange';
import { buildSigningPayload } from '../reference/backpack/backpack.signing';
import { BackpackOrderRequest, ENDPOINTS, INSTRUCTIONS } from '../reference/backpack/backpack.types';
import { DefaultStore } from '../../store/store.base';
import { ExchangeOptions, OrderSide, OrderTimeInForce, OrderType, PlaceOrderOpts } from '../../types';

const btcSymbol = 'BTC_USDC_PERP';
const solSymbol = 'SOL_USDC_PERP';

const makeLimit = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: btcSymbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 1.234,
  price: 100.1,
  timeInForce: OrderTimeInForce.GoodTillCancel,
  ...overrides,
});

const makeOk = (request: BackpackOrderRequest, id: string) => ({
  operation: 'Ok',
  id,
  clientId: request.clientId,
  symbol: request.symbol,
  side: request.side,
  orderType: request.orderType,
});

describe('Backpack conservative native order batching', () => {
  let exchange: BackpackExchange;
  let post: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    const store = new DefaultStore();
    store.update({
      markets: [btcSymbol, solSymbol].map((symbol) => ({
        id: symbol,
        symbol,
        base: symbol.slice(0, 3),
        quote: 'USDC',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
        contractSize: 1,
        takerFee: 0,
        makerFee: 0,
      })),
    });

    exchange = new BackpackExchange('backpack', { key: '', secret: '', testnet: false } as ExchangeOptions, store);
    post = vi.fn();
    exchange.xhr = { post } as any;
    (exchange as any).nextBatchClientId = vi.fn().mockReturnValueOnce(101).mockReturnValueOnce(202);
  });

  afterEach(() => {
    exchange.dispose();
  });

  it('matches the official per-row signing bytes and preserves an array auth body', () => {
    const rows = [
      { symbol: solSymbol, side: 'Bid', orderType: 'Limit', price: 141, quantity: 12 },
      { symbol: solSymbol, side: 'Bid', orderType: 'Limit', price: 140, quantity: 11 },
    ];

    expect(buildSigningPayload('orderExecute', rows, 1_750_793_021_519, 5000)).toBe(
      'instruction=orderExecute&orderType=Limit&price=141&quantity=12&side=Bid&symbol=SOL_USDC_PERP' +
        '&instruction=orderExecute&orderType=Limit&price=140&quantity=11&side=Bid&symbol=SOL_USDC_PERP' +
        '&timestamp=1750793021519&window=5000',
    );
    expect(collectAuthParams(undefined, rows)).toBe(rows);
    expect(collectAuthParams(undefined, rows)).not.toEqual({ 0: rows[0], 1: rows[1] });
    expect(getInstruction('POST', ENDPOINTS.ORDERS)).toBe(INSTRUCTIONS.ORDER_EXECUTE);
  });

  it('preflights and writes an exact pair, preserving ordinary attributes and decoding by clientId', async () => {
    post.mockImplementation(async (endpoint, requests: BackpackOrderRequest[]) => {
      expect(endpoint).toBe(ENDPOINTS.ORDERS);
      return { data: [makeOk(requests[1], 'exchange-second'), makeOk(requests[0], 'exchange-first')] };
    });

    const ids = await exchange.placeOrders([
      makeLimit({ timeInForce: OrderTimeInForce.PostOnly, reduceOnly: true }),
      makeLimit({
        symbol: solSymbol,
        side: OrderSide.Sell,
        amount: 2.345,
        price: 99.9,
        timeInForce: OrderTimeInForce.FillOrKill,
      }),
    ]);

    expect(ids).toEqual(['exchange-first', 'exchange-second']);
    expect(post).toHaveBeenCalledTimes(1);
    expect(post.mock.calls[0][1]).toEqual([
      {
        symbol: btcSymbol,
        side: 'Bid',
        orderType: 'Limit',
        price: '100.1',
        quantity: '1.234',
        timeInForce: 'GTC',
        postOnly: true,
        clientId: 101,
        reduceOnly: true,
      },
      {
        symbol: solSymbol,
        side: 'Ask',
        orderType: 'Limit',
        price: '99.9',
        quantity: '2.345',
        timeInForce: 'FOK',
        postOnly: false,
        clientId: 202,
      },
    ]);
  });

  it('uses one native pair and leaves an unmatched ordinary tail on the single path', async () => {
    post.mockImplementation(async (endpoint, body: BackpackOrderRequest | BackpackOrderRequest[]) => {
      if (endpoint === ENDPOINTS.ORDERS) {
        const requests = body as BackpackOrderRequest[];
        return { data: requests.map((request, index) => makeOk(request, `pair-${index}`)) };
      }
      return { data: { id: 'single-tail' } };
    });

    await expect(exchange.placeOrders([makeLimit(), makeLimit({ symbol: solSymbol }), makeLimit()])).resolves.toEqual([
      'pair-0',
      'pair-1',
      'single-tail',
    ]);
    expect(post.mock.calls.map(([endpoint]) => endpoint)).toEqual([ENDPOINTS.ORDERS, ENDPOINTS.ORDER]);
  });

  it.each([
    ['partial count', (requests: BackpackOrderRequest[]) => [makeOk(requests[0], 'first')]],
    [
      'non-Ok operation',
      (requests: BackpackOrderRequest[]) => [
        makeOk(requests[0], 'first'),
        { ...makeOk(requests[1], 'second'), operation: 'Err' },
      ],
    ],
    [
      'duplicate clientId',
      (requests: BackpackOrderRequest[]) => [makeOk(requests[0], 'first'), makeOk(requests[0], 'second')],
    ],
    [
      'unknown clientId',
      (requests: BackpackOrderRequest[]) => [
        makeOk(requests[0], 'first'),
        { ...makeOk(requests[1], 'second'), clientId: 303 },
      ],
    ],
    [
      'duplicate exchange id',
      (requests: BackpackOrderRequest[]) => [makeOk(requests[0], 'same'), makeOk(requests[1], 'same')],
    ],
    [
      'missing exchange id',
      (requests: BackpackOrderRequest[]) => [makeOk(requests[0], 'first'), { ...makeOk(requests[1], ''), id: '   ' }],
    ],
  ])('fails the whole pair without inventing ids for a %s response', async (_name, responseFactory) => {
    const emitAndLogError = vi.fn();
    exchange.emitAndLogError = emitAndLogError;
    post.mockImplementation(async (_endpoint, requests: BackpackOrderRequest[]) => ({
      data: responseFactory(requests),
    }));

    await expect(exchange.placeOrders([makeLimit(), makeLimit({ symbol: solSymbol })])).resolves.toEqual([]);
    expect(post).toHaveBeenCalledTimes(1);
    expect(emitAndLogError).toHaveBeenCalledTimes(1);
  });

  it('keeps invalid or oversize rows and their valid neighbors off the native batch path', async () => {
    post.mockResolvedValue({ data: { id: 'single-order' } });

    await expect(exchange.placeOrders([makeLimit({ symbol: 'MISSING_USDC_PERP' }), makeLimit()])).resolves.toEqual([
      'single-order',
    ]);
    expect(post.mock.calls.map(([endpoint]) => endpoint)).toEqual([ENDPOINTS.ORDER]);

    post.mockClear();
    await expect(exchange.placeOrders([makeLimit({ amount: 101 }), makeLimit({ symbol: solSymbol })])).resolves.toEqual(
      ['single-order', 'single-order'],
    );
    expect(post.mock.calls.map(([endpoint]) => endpoint)).toEqual([ENDPOINTS.ORDER, ENDPOINTS.ORDER]);
  });

  it('performs no write and surfaces the failure when unique uint32 generation fails', async () => {
    const emitAndLogError = vi.fn();
    exchange.emitAndLogError = emitAndLogError;

    (exchange as any).nextBatchClientId = vi.fn().mockReturnValue(101);
    await expect(exchange.placeOrders([makeLimit(), makeLimit({ symbol: solSymbol })])).resolves.toEqual([]);
    expect(post).not.toHaveBeenCalled();
    expect(emitAndLogError).toHaveBeenCalledTimes(1);
  });

  it('surfaces a native pair network failure while preserving continue semantics', async () => {
    const emitAndLogError = vi.fn();
    exchange.emitAndLogError = emitAndLogError;
    post.mockRejectedValue(new Error('temporary batch outage'));

    await expect(exchange.placeOrders([makeLimit(), makeLimit({ symbol: solSymbol })])).resolves.toEqual([]);
    expect(emitAndLogError).toHaveBeenCalledWith(expect.any(Error), 'Failed to place Backpack native order pair');
  });

  it('keeps MARKET, trigger, TP/SL composite, and one-order calls on the existing single endpoint', async () => {
    let id = 0;
    post.mockImplementation(async () => ({ data: { id: `single-${++id}` } }));

    const ids = await exchange.placeOrders([
      makeLimit(),
      { symbol: btcSymbol, type: OrderType.Market, side: OrderSide.Buy, amount: 1 },
      makeLimit({ triggerPrice: 95 }),
      makeLimit({ takeProfit: 110, stopLoss: 90 }),
      makeLimit({ close: true }),
    ]);

    expect(ids).toEqual(['single-1', 'single-2', 'single-3', 'single-4', 'single-5', 'single-6', 'single-7']);
    expect(post).toHaveBeenCalledTimes(7);
    expect(post.mock.calls.every(([endpoint]) => endpoint === ENDPOINTS.ORDER)).toBe(true);
  });
});
