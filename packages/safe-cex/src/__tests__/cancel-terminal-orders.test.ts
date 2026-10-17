import type { Market, Order, Position } from '../types';

import { describe, expect, it, vi } from 'vitest';

import { BaseExchange } from '../exchanges/base';
import { BinanceExchange } from '../exchanges/binance/binance.exchange';
import { ENDPOINTS as BINANCE_ENDPOINTS } from '../exchanges/binance/binance.types';
import { BitgetExchange } from '../exchanges/bitget/bitget.exchange';
import { ENDPOINTS as BITGET_ENDPOINTS } from '../exchanges/bitget/bitget.types';
import { BitunixExchange } from '../exchanges/bitunix/bitunix.exchange';
import { ENDPOINTS as BITUNIX_ENDPOINTS } from '../exchanges/bitunix/bitunix.types';
import { BlofinExchange } from '../exchanges/blofin/blofin.exchange';
import { ENDPOINTS as BLOFIN_ENDPOINTS } from '../exchanges/blofin/blofin.types';
import { LighterExchange } from '../exchanges/lighter/lighter.exchange';
import { OKXExchange } from '../exchanges/okx/okx.exchange';
import { ENDPOINTS as OKX_ENDPOINTS } from '../exchanges/okx/okx.types';
import { PhemexExchange } from '../exchanges/phemex/phemex.exchange';
import { ENDPOINTS as PHEMEX_ENDPOINTS } from '../exchanges/phemex/phemex.types';
import { DefaultStore } from '../store/store.base';
import { MarginType, OrderSide, OrderStatus, OrderType, PositionMode, PositionSide } from '../types';

const options = {
  key: 'test-key',
  secret: 'test-secret',
  passphrase: 'test-passphrase',
  testnet: false,
  tuning: { consoleLogging: false },
};

const market: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
};

const order = (id: string, status: OrderStatus, symbol = market.symbol): Order => ({
  id,
  symbol,
  status,
  type: status === OrderStatus.Closed ? OrderType.Market : OrderType.Limit,
  side: OrderSide.Buy,
  price: 50_000,
  amount: 1,
  filled: status === OrderStatus.Closed ? 1 : 0,
  remaining: status === OrderStatus.Open ? 1 : 0,
  reduceOnly: false,
  lastUpdate: 0,
  info: { posSide: 'Merged' },
});

// Each venue overrides cancelSymbolOrders; '101' is the filled order still in the store.
const venues = [
  {
    name: 'OKX',
    marketId: 'BTC-USDT-SWAP',
    createSubject(store: DefaultStore) {
      const exchange = new OKXExchange('okx', options, store);
      const cancel = vi
        .spyOn(exchange.xhr, 'post')
        .mockResolvedValue({ data: { code: '0', data: [{ sCode: '0', sMsg: '' }] } });
      return {
        exchange,
        cancel,
        expectedCall: [OKX_ENDPOINTS.CANCEL_ORDERS, [{ instId: 'BTC-USDT-SWAP', ordId: '202' }]],
      };
    },
  },
  {
    name: 'Bitget',
    marketId: 'BTCUSDT',
    createSubject(store: DefaultStore) {
      const exchange = new BitgetExchange('bitget', options, store);
      vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: { code: '40084', msg: 'Classic Account mode' } });
      const cancel = vi.spyOn(exchange.unlimitedXHR, 'post').mockResolvedValue({ data: { code: '00000' } });
      return {
        exchange,
        cancel,
        expectedCall: [
          BITGET_ENDPOINTS.CANCEL_ORDERS,
          { symbol: 'BTCUSDT', orderIdList: [{ orderId: '202' }], marginCoin: 'USDT', productType: 'usdt-futures' },
        ],
      };
    },
  },
  {
    name: 'Bitunix',
    marketId: 'BTC/USDT',
    createSubject(store: DefaultStore) {
      const exchange = new BitunixExchange('bitunix', options, store);
      const cancel = vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: { code: 0 } });
      return {
        exchange,
        cancel,
        expectedCall: [BITUNIX_ENDPOINTS.CANCEL_ORDER, { symbol: 'BTCUSDT', orderList: [{ orderId: '202' }] }],
      };
    },
  },
  {
    name: 'Blofin',
    marketId: 'BTC-USDT',
    createSubject(store: DefaultStore) {
      const exchange = new BlofinExchange('blofin', options, store);
      const cancel = vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: { code: '0' } });
      return {
        exchange,
        cancel,
        expectedCall: [BLOFIN_ENDPOINTS.CANCEL_ORDERS, [{ instId: 'BTC-USDT', orderId: '202' }]],
      };
    },
  },
  {
    name: 'Lighter',
    marketId: '1',
    createSubject(store: DefaultStore) {
      const exchange = new LighterExchange('lighter', options, store);
      const cancel = vi.fn().mockResolvedValue('cancel-tx');
      exchange.signerAdapter = {
        initialize: vi.fn(),
        createOrder: vi.fn(),
        createOrders: vi.fn(),
        createGroupedOrders: vi.fn(),
        cancelOrder: cancel,
        updateLeverage: vi.fn(),
        approveIntegrator: vi.fn(),
        getAuthToken: vi.fn(),
      };
      return { exchange, cancel, expectedCall: [{ orderId: 202n, marketId: 1 }] };
    },
  },
  {
    name: 'Phemex',
    marketId: 'BTCUSDT',
    createSubject(store: DefaultStore) {
      const exchange = new PhemexExchange('phemex', options, store);
      const cancel = vi.spyOn(exchange.unlimitedXHR, 'delete').mockResolvedValue({ data: { code: 0 } });
      return {
        exchange,
        cancel,
        expectedCall: [PHEMEX_ENDPOINTS.CANCEL_ORDER, { params: { symbol: 'BTCUSDT', posSide: 'Merged', orderID: '202' } }],
      };
    },
  },
];

describe('cancel selections skip orders the store already knows are terminal', () => {
  it.each(venues)('$name cancelSymbolOrders sends only the open order', async ({ marketId, createSubject }) => {
    const store = new DefaultStore();
    store.update({
      markets: [{ ...market, id: marketId }],
      orders: [order('101', OrderStatus.Closed), order('202', OrderStatus.Open)],
    });
    const { exchange, cancel, expectedCall } = createSubject(store);

    try {
      await exchange.cancelSymbolOrders('BTCUSDT');

      expect(cancel.mock.calls).toEqual([expectedCall]);
    } finally {
      exchange.dispose();
    }
  });

  it('base cancelAllOrders skips a symbol whose only orders are terminal', async () => {
    class RecordingExchange extends BaseExchange {
      canceled: string[][] = [];
      cancelOrders = async (orders: Order[]) => {
        this.canceled.push(orders.map((o) => o.id));
        await Promise.resolve();
      };
    }
    const store = new DefaultStore();
    store.update({
      markets: [market, { ...market, id: 'ETHUSDT', symbol: 'ETHUSDT', base: 'ETH' }],
      orders: [
        order('eth-filled', OrderStatus.Closed, 'ETHUSDT'),
        order('eth-canceled', OrderStatus.Canceled, 'ETHUSDT'),
        order('btc-filled', OrderStatus.Closed),
        order('btc-resting', OrderStatus.Open),
      ],
    });
    const exchange = new RecordingExchange('binance', options, store);

    await exchange.cancelAllOrders();

    expect(exchange.canceled).toEqual([['btc-resting']]);
  });

  it('Binance cancels only the open closePosition conflict before placing', async () => {
    const store = new DefaultStore();
    const position: Position = {
      id: 'BTCUSDT:long',
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      contracts: 1,
      entryPrice: 50_000,
      notional: 50_000,
      leverage: 10,
      unrealizedPnl: 0,
      realizedPnl: 0,
      liquidationPrice: 0,
      positionMode: PositionMode.OneWay,
      marginType: MarginType.Cross,
    };
    const closeStop = (row: Order): Order => ({
      ...row,
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 0,
      remaining: 0,
      close: true,
      reduceOnly: true,
    });
    store.update({
      markets: [market],
      positions: [position],
      orders: [closeStop(order('101', OrderStatus.Closed)), closeStop(order('202', OrderStatus.Open))],
    });
    const exchange = new BinanceExchange('binance', options, store);
    const cancel = vi.spyOn(exchange.xhr, 'delete').mockResolvedValue({ data: {} });
    vi.spyOn(exchange.unlimitedXHR, 'post').mockResolvedValue({ data: { algoId: '404' } });

    try {
      await exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        price: 49_000,
        amount: 0,
        close: true,
        reduceOnly: true,
      });

      expect(cancel.mock.calls).toEqual([[BINANCE_ENDPOINTS.ALGO_ORDER, { params: { algoId: '202' } }]]);
    } finally {
      exchange.dispose();
    }
  });
});
