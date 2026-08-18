import type { Mock } from 'vitest';
import { describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../exchanges/binance/binance.exchange';
import { ENDPOINTS as BINANCE_ENDPOINTS } from '../exchanges/binance/binance.types';
import { BitgetExchange } from '../exchanges/bitget/bitget.exchange';
import { ENDPOINTS as BITGET_ENDPOINTS } from '../exchanges/bitget/bitget.types';
import { BlofinExchange } from '../exchanges/blofin/blofin.exchange';
import { ENDPOINTS as BLOFIN_ENDPOINTS } from '../exchanges/blofin/blofin.types';
import { BybitExchange } from '../exchanges/bybit/bybit.exchange';
import { ENDPOINTS as BYBIT_ENDPOINTS } from '../exchanges/bybit/bybit.types';
import { OKXExchange } from '../exchanges/okx/okx.exchange';
import { ENDPOINTS as OKX_ENDPOINTS } from '../exchanges/okx/okx.types';
import type { Market } from '../types';

const market = (id = 'BTCUSDT'): Market => ({
  id,
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
});

const storeFor = (selected: Market) =>
  ({
    subscribe: vi.fn(),
    markets: [selected],
    marketsBySymbol: { [selected.symbol]: selected },
    tickersBySymbol: {},
    orders: [],
    positions: [],
  }) as never;

const credentials = { key: 'key', secret: 'secret', passphrase: 'passphrase' };

const replaceGet = (exchange: { xhr: unknown }, get: Mock) => {
  Object.defineProperty(exchange, 'xhr', { configurable: true, value: { get } });
};

describe('concrete exact client-order-ID readers', () => {
  it('queries Binance USD-M by translated symbol and exact wire ID', async () => {
    const selected = market('BTC/USDT:USDT');
    const exchange = new BinanceExchange('binance', credentials, storeFor(selected));
    const get = vi.fn().mockResolvedValue({ data: { orderId: 1 } });
    replaceGet(exchange, get);

    await expect(exchange.getOrderByClientOrderId(selected.symbol, 'x-s3vFFRQpcaller')).resolves.toEqual({ orderId: 1 });
    expect(get).toHaveBeenCalledWith(BINANCE_ENDPOINTS.ORDER, {
      params: { symbol: selected.symbol, origClientOrderId: 'x-s3vFFRQpcaller' },
    });
  });

  it('queries Bybit realtime then history with the bound linear market ID', async () => {
    const selected = market('BTCUSDT');
    const exchange = new BybitExchange('bybit', credentials, storeFor(selected));
    const empty = { retCode: 0, result: { category: 'linear', list: [] } };
    const history = { retCode: 0, result: { category: 'linear', list: [{ orderId: '1' }] } };
    const get = vi.fn().mockResolvedValueOnce({ data: empty }).mockResolvedValueOnce({ data: history });
    replaceGet(exchange, get);

    await expect(exchange.getOrderByClientOrderId(selected.symbol, 'caller-1')).resolves.toEqual(history);
    expect(get).toHaveBeenNthCalledWith(1, BYBIT_ENDPOINTS.UNFILLED_ORDERS, {
      params: { category: 'linear', symbol: selected.id, orderLinkId: 'caller-1', openOnly: 0 },
    });
    expect(get).toHaveBeenNthCalledWith(2, BYBIT_ENDPOINTS.USER_ORDERS, {
      params: { category: 'linear', symbol: selected.id, orderLinkId: 'caller-1' },
    });
  });

  it('queries OKX swap by instId and returns its HTTP absence envelope', async () => {
    const selected = market('BTC-USDT-SWAP');
    const exchange = new OKXExchange('okx', credentials, storeFor(selected));
    const absent = { code: '51603', data: [] };
    const get = vi.fn().mockRejectedValue({ response: { data: absent } });
    replaceGet(exchange, get);

    await expect(exchange.getOrderByClientOrderId(selected.symbol, 'e847386590ce4dBCcaller')).resolves.toEqual(absent);
    expect(get).toHaveBeenCalledWith(OKX_ENDPOINTS.ORDER, {
      params: { instId: selected.id, clOrdId: 'e847386590ce4dBCcaller' },
    });
  });

  it('queries proven-Classic Bitget V2 order detail by clientOid', async () => {
    const selected = market('BTCUSDT');
    const exchange = new BitgetExchange('bitget', credentials, storeFor(selected));
    const response = { code: '00000', data: { orderId: '1' } };
    const get = vi.fn().mockResolvedValue({ data: response });
    replaceGet(exchange, get);

    await expect(exchange.getOrderByClientOrderId(selected.symbol, 'caller-1')).resolves.toEqual(response);
    expect(get).toHaveBeenCalledWith(BITGET_ENDPOINTS.ORDER_DETAIL, {
      params: { symbol: selected.id, productType: exchange.apiProductType, clientOid: 'caller-1' },
    });
  });

  it('queries BloFin swap by instId and exact composed wire ID', async () => {
    const selected = market('BTC-USDT');
    const exchange = new BlofinExchange('blofin', credentials, storeFor(selected));
    const response = { code: '0', data: { orderId: '1' } };
    const get = vi.fn().mockResolvedValue({ data: response });
    replaceGet(exchange, get);

    await expect(exchange.getOrderByClientOrderId(selected.symbol, 'b3cdedc0a20880bacaller')).resolves.toEqual(response);
    expect(get).toHaveBeenCalledWith(BLOFIN_ENDPOINTS.ORDER_DETAIL, {
      params: { instId: selected.id, clientOrderId: 'b3cdedc0a20880bacaller' },
    });
  });

  it('rejects invalid lookup identity before any request', async () => {
    const selected = market();
    const exchange = new BybitExchange('bybit', credentials, storeFor(selected));
    const get = vi.fn();
    replaceGet(exchange, get);

    await expect(exchange.getOrderByClientOrderId(selected.symbol, '')).rejects.toThrow('wire client order ID');
    expect(get).not.toHaveBeenCalled();
  });
});
