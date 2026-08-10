import type { Axios } from 'axios';
import type { ExchangeOptions, Market, PlaceOrderOpts } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { OKXExchange } from '../../exchanges/okx/okx.exchange';
import { BROKER_ID, ENDPOINTS } from '../../exchanges/okx/okx.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, PositionMode } from '../../types';

const options: ExchangeOptions = {
  key: 'test-key',
  secret: 'test-secret',
  passphrase: 'test-passphrase',
  testnet: false,
  tuning: { consoleLogging: false },
};

const createMarket = (amountStep: number, maxAmount: number): Market => ({
  id: 'BTC-USDT-SWAP',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: amountStep, price: 0.1 },
  limits: {
    amount: { min: amountStep, max: maxAmount },
    leverage: { min: 1, max: 100 },
  },
  contractSize: 1,
});

const limitOrder = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'BTCUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 5,
  price: 50_000,
  ...overrides,
});

describe('OKX caller clientOrderId broker prefix composition', () => {
  const BUDGET_MESSAGE =
    'OKX clientOrderId must be 1 to 16 case-sensitive alphanumeric characters (OKX reserves 16 of 32 chars for the Tealstreet broker prefix)';

  const createWireExchange = () => {
    const store = new DefaultStore();
    store.update({ markets: [createMarket(1, 10)] });
    const exchange = new OKXExchange('okx', options, store);
    exchange._accountPositionMode = PositionMode.OneWay;
    exchange._accountLevel = 2;
    const post = vi.fn().mockResolvedValue({ data: { data: [{ ordId: 'native-order' }] } });
    // partial xhr stand-in: only `post` is exercised in these tests
    exchange.xhr = { post } as unknown as Axios;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    return { exchange, post, emitAndLogError };
  };

  it('composes the broker prefix onto a caller clientOrderId on the wire', async () => {
    const { exchange, post } = createWireExchange();

    await exchange.placeOrder(limitOrder({ clientOrderId: 'CallerId1' }));

    expect(post).toHaveBeenCalledOnce();
    expect(post.mock.calls[0][0]).toBe(ENDPOINTS.PLACE_ORDERS);
    const payloads = post.mock.calls[0][1] as Array<Record<string, unknown>>;
    expect(payloads).toHaveLength(1);
    expect(payloads[0].clOrdId).toBe(BROKER_ID + 'CallerId1');
  });

  it('accepts a 16-char caller ID, composing to the 32-char venue maximum', async () => {
    const { exchange, post } = createWireExchange();

    await exchange.placeOrder(limitOrder({ clientOrderId: 'A'.repeat(16) }));

    const payloads = post.mock.calls[0][1] as Array<Record<string, unknown>>;
    expect(payloads[0].clOrdId).toBe(BROKER_ID + 'A'.repeat(16));
    expect(String(payloads[0].clOrdId)).toHaveLength(32);
  });

  it('rejects a 17-char caller ID before any request', async () => {
    const { exchange, post, emitAndLogError } = createWireExchange();

    await expect(exchange.placeOrder(limitOrder({ clientOrderId: 'A'.repeat(17) }))).rejects.toThrow(BUDGET_MESSAGE);

    expect(emitAndLogError).toHaveBeenCalledWith(BUDGET_MESSAGE);
    expect(post).not.toHaveBeenCalled();
  });

  it('keeps generated prefixed IDs within the venue maximum when no caller ID is provided', async () => {
    const { exchange, post } = createWireExchange();

    await exchange.placeOrder(limitOrder());

    const payloads = post.mock.calls[0][1] as Array<Record<string, unknown>>;
    expect(String(payloads[0].clOrdId).startsWith(BROKER_ID)).toBe(true);
    expect(String(payloads[0].clOrdId).length).toBeLessThanOrEqual(32);
  });
});
