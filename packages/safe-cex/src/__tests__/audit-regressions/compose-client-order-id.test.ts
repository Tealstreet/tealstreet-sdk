import type { Axios } from 'axios';
import type { ExchangeOptions, Market, PlaceOrderOpts } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { composeClientOrderId } from '../../composeClientOrderId';
import { BinanceExchange } from '../../exchanges/binance/binance.exchange';
import { BitgetExchange } from '../../exchanges/bitget/bitget.exchange';
import { BlofinExchange } from '../../exchanges/blofin/blofin.exchange';
import { OKXExchange } from '../../exchanges/okx/okx.exchange';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderType, PositionMode } from '../../types';

const options: ExchangeOptions = {
  key: 'key',
  secret: 'secret',
  passphrase: 'passphrase',
  testnet: false,
  tuning: { consoleLogging: false },
};

const market = (overrides: Partial<Market> = {}): Market => ({
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 10 }, leverage: { min: 1, max: 100 } },
  ...overrides,
});

const limitOrder = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'BTCUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 1,
  price: 50_000,
  ...overrides,
});

describe('composeClientOrderId matches the wire ID adapters emit', () => {
  it('binance USD-M: wire newClientOrderId is the composed caller ID', async () => {
    const store = new DefaultStore();
    store.update({ markets: [market()] });
    const exchange = new BinanceExchange('binance', options, store);
    const post = vi.fn().mockResolvedValue({ data: { orderId: 1 } });
    // partial xhr stand-in: only `post` is exercised in this test
    exchange.unlimitedXHR = { post } as unknown as Axios;

    await exchange.placeOrder(limitOrder({ clientOrderId: 'Caller1' }));

    const body = post.mock.calls[0][1] as Record<string, unknown>;
    expect(body.newClientOrderId).toBe(composeClientOrderId('binance', 'Caller1'));
    expect(body.newClientOrderId).toBe('x-s3vFFRQpCaller1');
  });

  it.each(['okx', 'okxeu'] as const)('%s: wire clOrdId is the composed caller ID', async (name) => {
    const store = new DefaultStore();
    store.update({
      markets: [
        market({
          id: 'BTC-USDT-SWAP',
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 10 }, leverage: { min: 1, max: 100 } },
          contractSize: 1,
        }),
      ],
    });
    const exchange = new OKXExchange(name, options, store);
    exchange._accountPositionMode = PositionMode.OneWay;
    exchange._accountLevel = 2;
    const post = vi.fn().mockResolvedValue({ data: { data: [{ ordId: 'native-order' }] } });
    // partial xhr stand-in: only `post` is exercised in this test
    exchange.xhr = { post } as unknown as Axios;

    await exchange.placeOrder(limitOrder({ amount: 5, clientOrderId: 'Caller1' }));

    const payloads = post.mock.calls[0][1] as Array<Record<string, unknown>>;
    expect(payloads[0].clOrdId).toBe(composeClientOrderId(name, 'Caller1'));
    expect(payloads[0].clOrdId).toBe('e847386590ce4dBCCaller1');
  });

  it('blofin: wire clientOrderId is the composed caller ID', async () => {
    const store = new DefaultStore();
    store.update({
      markets: [
        market({
          id: 'BTC-USDT',
          precision: { amount: 1, price: 0.1 },
          limits: { amount: { min: 1, max: 5 }, leverage: { min: 1, max: 100 } },
          info: { lotSize: '1', minSize: '1', contractValue: '0.001' },
        }),
      ],
    });
    const exchange = new BlofinExchange('blofin', options, store);
    vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
    vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
    const post = vi.fn().mockResolvedValue({ data: { code: '0', msg: 'success', data: [{ orderId: 'order-1' }] } });
    // partial xhr stand-in: only `post` is exercised in this test
    exchange.xhr = { post } as unknown as Axios;

    await exchange.placeOrder(limitOrder({ amount: 2, clientOrderId: 'Caller1' }));

    const payloads = post.mock.calls[0][1] as Array<Record<string, unknown>>;
    expect(payloads[0].clientOrderId).toBe(composeClientOrderId('blofin', 'Caller1'));
    expect(payloads[0].clientOrderId).toBe('b3cdedc0a20880baCaller1');
  });

  it('bitget: wire clientOid is the bare caller ID (attribution rides the channel header)', async () => {
    const store = new DefaultStore();
    store.update({ markets: [market()] });
    const exchange = new BitgetExchange('bitget', options, store);
    vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
    vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
    const post = vi.fn().mockResolvedValue({ data: { data: { successList: [{ orderId: 'order-1' }] } } });
    // partial xhr stand-in: only `post` is exercised in this test
    exchange.unlimitedXHR = { post } as unknown as Axios;

    await exchange.placeOrder(limitOrder({ clientOrderId: 'Caller1' }));

    const body = post.mock.calls[0][1] as { orderList: Array<Record<string, unknown>> };
    expect(body.orderList[0].clientOid).toBe(composeClientOrderId('bitget', 'Caller1'));
    expect(body.orderList[0].clientOid).toBe('Caller1');
  });

  it.each(['bybit', 'binancepm', 'binancespot', 'bitget'] as const)(
    '%s: caller ID passes through the helper unmodified',
    (name) => {
      expect(composeClientOrderId(name, 'Caller1')).toBe('Caller1');
    },
  );
});
