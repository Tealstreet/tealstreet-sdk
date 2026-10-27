import type {
  WooV3AccountInfo,
  WooV3Balance,
  WooV3ListenKeyRequest,
  WooV3ListenKeyResult,
  WooV3PlaceOrderRequest,
  WooV3PlaceOrderResult,
  WooV3Position,
  WooV3Response,
} from '../reference/woo/woo.v3.contracts';

import { describe, expect, it } from 'vitest';

import {
  WOO_V3_ENDPOINTS,
  WOO_V3_MIGRATION_GATES,
  WOO_V3_MIGRATION_ROUTES,
  WOO_V3_RUNTIME_STATUS,
  WOO_V3_WEBSOCKET_MIGRATION,
  WOO_V3_WS_URL,
} from '../reference/woo/woo.v3.contracts';
import { prepareWooV3Request } from '../reference/woo/woo.v3.request';

const DOCUMENTED_AUTH_FIXTURE = {
  apiKey: 'AbmyVJGUpN064ks5ELjLfA==',
  apiSecret: 'QHKRXHPAW1MC9YGZMAT8YDJG2HPR',
  timestamp: 1_578_565_539_808,
} as const;

const accountFixture = {
  success: true,
  data: {
    applicationId: 'safe-cex',
    account: 'demo',
    alias: 'demo',
    accountMode: 'FUTURES',
    positionMode: 'ONE_WAY',
    leverage: 10,
    marginRatio: '1',
    totalCollateral: '100.25',
    totalAccountValue: '101.50',
    accountType: 'Main',
  },
  timestamp: 1_693_663_264_324,
} as const satisfies WooV3Response<WooV3AccountInfo>;

const balanceFixture = {
  success: true,
  data: {
    holding: [
      {
        token: 'USDT',
        holding: '100.25',
        frozen: '0',
        availableBalance: '100.25',
        pendingShortQty: '0',
        pendingLongQty: '0',
        averageOpenPrice: '0',
        markPrice: '1',
        timestamp: 1_693_663_746_259,
      },
    ],
  },
  timestamp: 1_693_663_746_259,
} as const satisfies WooV3Response<{ holding: readonly WooV3Balance[] }>;

const positionsFixture = {
  success: true,
  data: {
    positions: [
      {
        symbol: 'PERP_BTC_USDT',
        holding: '1.0234',
        positionSide: 'BOTH',
        pendingLongQty: '0.0',
        pendingShortQty: '0.0',
        settlePrice: '27903.2',
        averageOpenPrice: '27903.2',
        markPrice: '27903.2',
        estLiqPrice: '0.0',
        timestamp: 1_696_663_260_695,
      },
    ],
  },
  timestamp: 1_696_663_264_324,
} as const satisfies WooV3Response<{ positions: readonly WooV3Position[] }>;

const placeOrderFixture = {
  request: {
    symbol: 'PERP_BTC_USDT',
    side: 'BUY',
    type: 'LIMIT',
    price: '100',
    quantity: '0.1',
    reduceOnly: false,
  } satisfies WooV3PlaceOrderRequest,
  response: {
    success: true,
    data: {
      orderId: 123,
      type: 'LIMIT',
      price: '100',
      quantity: '0.1',
      amount: '10',
      clientOrderId: 456,
      bidAskLevel: 0,
    },
    timestamp: 1_578_565_539_808,
  } satisfies WooV3Response<WooV3PlaceOrderResult>,
} as const;

const listenKeyFixture = {
  request: { type: 'WEBSOCKET' } satisfies WooV3ListenKeyRequest,
  response: {
    success: true,
    data: { authKey: 'fixture-listen-key', expiredTime: 1_578_651_939_808 },
    timestamp: 1_578_565_539_808,
  } satisfies WooV3Response<WooV3ListenKeyResult>,
} as const;

describe('WooX V3 migration foundation', () => {
  it('keeps every migration family hard-disabled and disconnected from runtime', () => {
    expect(WOO_V3_RUNTIME_STATUS).toBe('scaffold-only');
    expect(Object.values(WOO_V3_MIGRATION_GATES)).toEqual([false, false, false, false, false, false, false, false]);
    expect(WOO_V3_MIGRATION_ROUTES.every(({ gate }) => WOO_V3_MIGRATION_GATES[gate] === false)).toBe(true);
  });

  it('locks the official namespaced REST and websocket routes', () => {
    expect(WOO_V3_ENDPOINTS).toMatchObject({
      PUBLIC_INSTRUMENTS: '/v3/public/instruments',
      TRADE_ORDER: '/v3/trade/order',
      TRADE_ALGO_ORDER: '/v3/trade/algoOrder',
      ACCOUNT_INFO: '/v3/account/info',
      ASSET_BALANCES: '/v3/asset/balances',
      FUTURES_POSITIONS: '/v3/futures/positions',
      FUTURES_POSITION_MODE: '/v3/futures/positionMode',
      FUTURES_LEVERAGE: '/v3/futures/leverage',
    });
    expect(WOO_V3_WS_URL.private.livenet).toBe('wss://wss.woox.io/v3/private');
    expect(WOO_V3_WEBSOCKET_MIGRATION.private).toEqual({
      legacyUrl: 'wss://wss.woox.io/v2/ws/private/stream/:applicationId',
      v3Url: 'wss://wss.woox.io/v3/private?key=:listenKey',
      listenKeyEndpoint: '/v3/account/listenKey',
      authentication: 'listen-key-query',
      gate: 'privateWebsocketParity',
    });
  });

  it('records method changes and release gates in the migration map', () => {
    expect(WOO_V3_MIGRATION_ROUTES).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: 'place-order',
          legacyMethod: 'POST',
          legacyPath: '/v1/order',
          v3Method: 'POST',
          v3Path: '/v3/trade/order',
          gate: 'regularWriteDemoLifecycle',
        }),
        expect.objectContaining({
          id: 'position-mode',
          legacyMethod: 'POST',
          v3Method: 'PUT',
          v3Path: '/v3/futures/positionMode',
          gate: 'accountWriteDemoLifecycle',
        }),
        expect.objectContaining({
          id: 'historical-klines',
          v3Path: '/v3/public/klineHistory',
          gate: 'publicReadParity',
        }),
      ]),
    );
  });

  it('keeps documented decimal fields as strings in response fixtures', () => {
    expect(accountFixture.data.totalCollateral).toBe('100.25');
    expect(balanceFixture.data.holding[0]?.availableBalance).toBe('100.25');
    expect(positionsFixture.data.positions[0]?.holding).toBe('1.0234');
    expect(placeOrderFixture.response.data.price).toBe('100');
    expect(listenKeyFixture.response.data.authKey).toBe('fixture-listen-key');
  });

  it('normalizes and signs GET query parameters in the request path', () => {
    const request = prepareWooV3Request(
      {
        method: 'GET',
        path: WOO_V3_ENDPOINTS.TRADE_ORDER,
        query: { symbol: 'PERP_BTC_USDT', orderId: 123 },
      },
      DOCUMENTED_AUTH_FIXTURE,
    );

    expect(request).toEqual({
      method: 'GET',
      path: '/v3/trade/order?orderId=123&symbol=PERP_BTC_USDT',
      headers: {
        'x-api-key': DOCUMENTED_AUTH_FIXTURE.apiKey,
        'x-api-timestamp': '1578565539808',
        'x-api-signature': '753bd76f14e5a4ef3e4c2736af554c09674711d4a52bbbbcc05401954ceb1483',
      },
      signaturePayload: '1578565539808GET/v3/trade/order?orderId=123&symbol=PERP_BTC_USDT',
    });
  });

  it('signs the exact JSON body sent by a POST descriptor', () => {
    const request = prepareWooV3Request(
      {
        method: 'POST',
        path: WOO_V3_ENDPOINTS.TRADE_ORDER,
        body: placeOrderFixture.request,
      },
      DOCUMENTED_AUTH_FIXTURE,
    );

    expect(request.body).toBe(
      '{"symbol":"PERP_BTC_USDT","side":"BUY","type":"LIMIT","price":"100","quantity":"0.1","reduceOnly":false}',
    );
    expect(request.signaturePayload).toBe(`1578565539808POST/v3/trade/order${request.body}`);
    expect(request.headers).toMatchObject({
      'content-type': 'application/json',
      'x-api-signature': '4219f1722523f2f6c900b74e5c18993a8a4c567d058c8202fab557bf051a4ca1',
    });
  });

  it('builds public fixtures without credentials and rejects unsafe parameter placement', () => {
    expect(
      prepareWooV3Request({
        method: 'GET',
        path: WOO_V3_ENDPOINTS.PUBLIC_INSTRUMENTS,
        query: { symbol: 'PERP_BTC_USDT' },
      }),
    ).toEqual({
      method: 'GET',
      path: '/v3/public/instruments?symbol=PERP_BTC_USDT',
      headers: {},
    });
    expect(
      prepareWooV3Request(
        {
          method: 'GET',
          path: WOO_V3_ENDPOINTS.PUBLIC_INSTRUMENTS,
        },
        DOCUMENTED_AUTH_FIXTURE,
      ).headers,
    ).toEqual({});

    expect(() =>
      prepareWooV3Request({
        method: 'GET',
        path: WOO_V3_ENDPOINTS.ACCOUNT_INFO,
      }),
    ).toThrow('require explicit credentials');
    expect(() =>
      prepareWooV3Request({
        method: 'DELETE',
        path: WOO_V3_ENDPOINTS.TRADE_ORDER,
        body: { orderId: 123 },
      }),
    ).toThrow('query string');
    expect(() =>
      prepareWooV3Request({
        method: 'POST',
        path: WOO_V3_ENDPOINTS.TRADE_ORDER,
        query: { symbol: 'PERP_BTC_USDT' },
      }),
    ).toThrow('JSON body');
  });
});
