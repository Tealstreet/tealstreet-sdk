import { afterEach, describe, expect, it, vi } from 'vitest';

import { Signer as GrvtSigner } from '@tealstreet/grvt-ts-sdk';
import { GrvtExchange } from '../reference/grvt/grvt.exchange';
import { grvtNanoseconds, grvtScaledUint64, grvtUint64 } from '../reference/grvt/grvt.integer';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

afterEach(() => vi.restoreAllMocks());

describe('GRVT signing integer precision', () => {
  it.each([
    ['0.03', 9, '30000000'],
    ['1.2345678901234567', 18, '1234567890123456700'],
    ['18446744073.709551615', 9, '18446744073709551615'],
    ['1e-9', 9, '1'],
    ['1.2300', 2, '123'],
    ['1000', -3, '1'],
    ['1e3', -3, '1'],
    ['0e-999', 0, '0'],
  ])('scales the exact REST decimal %s with exponent %s', (value, decimals, expected) => {
    expect(grvtScaledUint64(value, decimals)).toBe(expected);
  });

  it.each([
    '',
    '-1',
    '+1',
    'NaN',
    'Infinity',
    ' 1',
    '1 ',
    '.',
    '1.2.3',
    '1e',
    '1e1e1',
    '0.0000000001',
    '18446744073709551616',
  ])('refuses malformed, fractional, or overflowing native units: %s', (value) => {
    expect(() => grvtScaledUint64(value, 9)).toThrow();
  });

  it('preserves full uint64 IDs and bounds signed nanosecond timestamps', () => {
    expect(grvtUint64('9007199254740993')).toBe('9007199254740993');
    expect(grvtUint64('18446744073709551615')).toBe('18446744073709551615');
    for (const value of ['01', '-1', '1.0', '18446744073709551616']) expect(() => grvtUint64(value)).toThrow();
    expect(grvtNanoseconds(1_700_000_000_001)).toBe('1700000000001000000');
    expect(() => grvtNanoseconds(Number.MAX_SAFE_INTEGER)).toThrow();
    expect(() => grvtNanoseconds(0.1)).toThrow();
  });

  it('signs the exact account and REST leg units instead of rounded Number products', async () => {
    const store = new DefaultStore();
    store.update({
      markets: [
        {
          id: 'BTC_USDT_Perp',
          symbol: 'BTC_USDT_PERP',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 0.0000000000000001, price: 0.1 },
          limits: { amount: { min: 0.0000000000000001, max: 100 }, leverage: { min: 1, max: 100 } },
          info: { ih: '0x030501', bd: 18, qd: 6 },
        },
      ] as any,
    });
    const exchange = new GrvtExchange(
      'grvt' as any,
      { secret: `0x${'01'.repeat(32)}`, tuning: { consoleLogging: false } } as any,
      store,
    );
    exchange.grvtTradingAccountId = '9007199254740993';
    const createOrder = vi.fn().mockResolvedValue({ result: { order_id: '0x00' } });
    (exchange as any).grvtClient = { tdgClient: { createOrder }, authenticatedEndpoint: vi.fn().mockResolvedValue({}) };
    vi.spyOn(exchange as any, 'getBuilderInfo').mockResolvedValue({
      builder: '0xee8f3559f298fffa79b8253437969d15b5dccff3',
      builderFee: '0.03',
      builderFeeInt: 300,
    });
    vi.spyOn(Date, 'now').mockReturnValue(1_700_000_000_001);
    const sign = vi.spyOn(GrvtSigner, 'sign');

    await exchange.placeOrder({
      symbol: 'BTC_USDT_PERP',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1.2345678901234567,
      price: 100,
    });

    const typed = sign.mock.calls[0][1] as {
      message: { subAccountID: string; legs: Array<{ contractSize: string; limitPrice: string }>; expiration: string };
    };
    const order = createOrder.mock.calls[0][0].order;
    expect(typed.message.subAccountID).toBe('9007199254740993');
    expect(order.sub_account_id).toBe(typed.message.subAccountID);
    expect(typed.message.legs[0].contractSize).toBe(grvtScaledUint64(order.legs[0].size, 18));
    expect(typed.message.legs[0].contractSize).toBe('1234567890123456700');
    expect(typed.message.legs[0].limitPrice).toBe('100000000000');
    expect(typed.message.expiration).toBe('1700003600001000000');
    expect(order.signature.expiration).toBe(typed.message.expiration);
    expect(order.metadata.create_time).toBe('1700000000001000000');
  });
});
