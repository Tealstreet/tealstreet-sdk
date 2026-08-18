import type { AxiosAdapter } from 'axios';
import { AxiosError, AxiosHeaders } from 'axios';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { composeClientOrderId } from '../composeClientOrderId';
import { BinanceExchange } from '../exchanges/binance/binance.exchange';
import { createAPI as createBinanceAPI } from '../exchanges/binance/binance.api';
import { ENDPOINTS as BINANCE_ENDPOINTS } from '../exchanges/binance/binance.types';
import { BitgetExchange } from '../exchanges/bitget/bitget.exchange';
import { createAPI as createBitgetAPI } from '../exchanges/bitget/bitget.api';
import { ENDPOINTS as BITGET_ENDPOINTS } from '../exchanges/bitget/bitget.types';
import { BlofinExchange } from '../exchanges/blofin/blofin.exchange';
import { createAPI as createBlofinAPI } from '../exchanges/blofin/blofin.api';
import { ENDPOINTS as BLOFIN_ENDPOINTS } from '../exchanges/blofin/blofin.types';
import { BybitExchange } from '../exchanges/bybit/bybit.exchange';
import { createAPI as createBybitAPI } from '../exchanges/bybit/bybit.api';
import { ENDPOINTS as BYBIT_ENDPOINTS } from '../exchanges/bybit/bybit.types';
import { OKXExchange } from '../exchanges/okx/okx.exchange';
import { createAPI as createOkxAPI } from '../exchanges/okx/okx.api';
import { ENDPOINTS as OKX_ENDPOINTS } from '../exchanges/okx/okx.types';
import type { ExchangeOptions, Market } from '../types';
import { virtualClock } from '../utils/virtualClock';

interface RequestSnapshot {
  readonly data: string | undefined;
  readonly params: Readonly<Record<string, unknown>> | undefined;
}

const selectedMarket: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
};

const store = {
  subscribe: vi.fn(),
  markets: [selectedMarket],
  marketsBySymbol: { [selectedMarket.symbol]: selectedMarket },
  tickersBySymbol: {},
  orders: [],
  positions: [],
} as never;

const options: ExchangeOptions = { key: 'key', secret: 'secret', passphrase: 'passphrase' };

const retryProbe = (successData: unknown) => {
  const snapshots: RequestSnapshot[] = [];
  const adapter: AxiosAdapter = async (config) => {
    const params =
      typeof config.params === 'object' && config.params !== null
        ? { ...config.params }
        : undefined;
    snapshots.push({
      data:
        config.data === undefined
          ? undefined
          : typeof config.data === 'string'
            ? config.data
            : JSON.stringify(config.data),
      params,
    });
    if (snapshots.length === 1) {
      throw new AxiosError('socket reset', 'ECONNRESET', config, {});
    }
    return {
      data: successData,
      status: 200,
      statusText: 'OK',
      headers: new AxiosHeaders(),
      config,
    };
  };
  return { adapter, snapshots };
};

const parsedBody = (snapshot: RequestSnapshot): Readonly<Record<string, unknown>> => {
  if (snapshot.data === undefined) throw new Error('Expected JSON request body');
  const parsed: unknown = JSON.parse(snapshot.data);
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    throw new Error('Expected JSON object request body');
  }
  return parsed as Readonly<Record<string, unknown>>;
};

const expectStableBodyIdentity = (
  snapshots: readonly RequestSnapshot[],
  field: string,
  expectedWireId: string,
) => {
  expect(snapshots).toHaveLength(2);
  expect(snapshots[0]?.data).toBe(snapshots[1]?.data);
  expect(parsedBody(snapshots[0]!)[field]).toBe(expectedWireId);
};

afterEach(() => {
  vi.restoreAllMocks();
});

describe('stock mutation retries preserve order identity', () => {
  it('preserves Binance order parameters and composed newClientOrderId', async () => {
    const exchange = new BinanceExchange('binance', options, store);
    const api = createBinanceAPI(options, exchange);
    const wireId = composeClientOrderId('binance', 'caller');
    const probe = retryProbe({ orderId: 1, clientOrderId: wireId });
    api.defaults.adapter = probe.adapter;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValueOnce(1000).mockReturnValue(2000);

    await api.post(BINANCE_ENDPOINTS.ORDER, { symbol: selectedMarket.id, side: 'BUY', newClientOrderId: wireId });

    expect(probe.snapshots).toHaveLength(2);
    const first = probe.snapshots[0]?.params;
    const second = probe.snapshots[1]?.params;
    expect(first?.newClientOrderId).toBe(wireId);
    expect(second?.newClientOrderId).toBe(wireId);
    expect({ symbol: first?.symbol, side: first?.side }).toEqual({ symbol: second?.symbol, side: second?.side });
  });

  it('preserves Bybit JSON body and composed orderLinkId', async () => {
    const exchange = new BybitExchange('bybit', options, store);
    const api = createBybitAPI(options, exchange);
    const wireId = composeClientOrderId('bybit', 'caller');
    const probe = retryProbe({ retCode: 0, retMsg: 'OK', result: { orderId: '1', orderLinkId: wireId } });
    api.defaults.adapter = probe.adapter;

    await api.post(BYBIT_ENDPOINTS.CREATE_ORDER, { symbol: selectedMarket.id, orderLinkId: wireId, qty: '1' });
    expectStableBodyIdentity(probe.snapshots, 'orderLinkId', wireId);
  });

  it('preserves OKX JSON body and composed clOrdId', async () => {
    const exchange = new OKXExchange('okx', options, store);
    const api = createOkxAPI(options, exchange);
    const wireId = composeClientOrderId('okx', 'caller');
    const probe = retryProbe({ code: '0', data: [{ sCode: '0', ordId: '1', clOrdId: wireId }] });
    api.defaults.adapter = probe.adapter;

    await api.post(OKX_ENDPOINTS.ORDER, { instId: 'BTC-USDT-SWAP', clOrdId: wireId, sz: '1' });
    expectStableBodyIdentity(probe.snapshots, 'clOrdId', wireId);
  });

  it('preserves Bitget JSON body and clientOid', async () => {
    const exchange = new BitgetExchange('bitget', options, store);
    const api = createBitgetAPI(options, exchange);
    const wireId = composeClientOrderId('bitget', 'caller');
    const probe = retryProbe({ code: '00000', data: { orderId: '1', clientOid: wireId } });
    api.defaults.adapter = probe.adapter;

    await api.post(BITGET_ENDPOINTS.PLACE_ORDER, { symbol: selectedMarket.id, clientOid: wireId, size: '1' });
    expectStableBodyIdentity(probe.snapshots, 'clientOid', wireId);
  });

  it('preserves BloFin JSON body and composed clientOrderId', async () => {
    const exchange = new BlofinExchange('blofin', options, store);
    const api = createBlofinAPI(options, exchange);
    const wireId = composeClientOrderId('blofin', 'caller');
    const probe = retryProbe({ code: '0', data: [{ code: '0', orderId: '1', clientOrderId: wireId }] });
    api.defaults.adapter = probe.adapter;

    await api.post(BLOFIN_ENDPOINTS.PLACE_ORDERS, { instId: 'BTC-USDT', clientOrderId: wireId, size: '1' });
    expectStableBodyIdentity(probe.snapshots, 'clientOrderId', wireId);
  });
});
