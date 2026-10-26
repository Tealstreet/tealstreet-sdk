import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { WOO_V3_MIGRATION_GATES } from '../reference/woo/woo.v3.contracts';
import type { WooV3RegularOrder } from '../reference/woo/woo.v3.private';
import {
  buildWooV3RegularAmendRequest,
  buildWooV3RegularCancelPendingRequest,
  buildWooV3RegularCancelRequest,
  buildWooV3RegularPlaceRequest,
  decodeWooV3RegularAmendResponse,
  decodeWooV3RegularCancelPendingResponse,
  decodeWooV3RegularCancelResponse,
  decodeWooV3RegularPlaceResponse,
  WOO_V3_REGULAR_WRITE_PARITY,
  WooV3WriteResponseError,
} from '../reference/woo/woo.v3.regular-writes';
import { prepareWooV3Request } from '../reference/woo/woo.v3.request';

const auth = {
  apiKey: 'AbmyVJGUpN064ks5ELjLfA==',
  apiSecret: 'QHKRXHPAW1MC9YGZMAT8YDJG2HPR',
  timestamp: 1_578_565_539_808,
} as const;

const currentOrder = (overrides: Partial<WooV3RegularOrder> = {}): WooV3RegularOrder => ({
  symbol: 'PERP_BTC_USDT',
  status: 'NEW',
  side: 'BUY',
  positionSide: 'BOTH',
  createdTime: 1_578_565_539_808,
  orderId: 135,
  orderTag: 'default',
  price: '62000',
  type: 'LIMIT',
  quantity: '0.1',
  amount: null,
  visible: '0.1',
  executed: '0',
  totalFee: '0',
  feeAsset: '',
  totalRebate: 0,
  rebateCurrency: '',
  clientOrderId: 42,
  reduceOnly: false,
  averageExecutedPrice: '0',
  ...overrides,
});

const success = (data: Record<string, unknown>) => ({ success: true, timestamp: 1_725_000_000_000, data });

describe('WooX V3 inert regular-order writes', () => {
  it('builds and signs an exact hedged post-only futures placement in base units', () => {
    const descriptor = buildWooV3RegularPlaceRequest({
      symbol: 'PERP_BTC_USDT',
      side: 'SELL',
      positionMode: 'HEDGE_MODE',
      type: 'LIMIT',
      timeInForce: 'POST_ONLY',
      quantity: '0.1000',
      price: '62123.5',
      reduceOnly: true,
      clientOrderId: 456,
      orderTag: 'safe-cex',
      visibleQuantity: '0.05',
      postOnlyAdjusted: false,
    });

    expect(descriptor).toEqual({
      method: 'POST',
      path: '/v3/trade/order',
      body: {
        symbol: 'PERP_BTC_USDT',
        side: 'SELL',
        positionSide: 'LONG',
        type: 'POST_ONLY',
        clientOrderId: 456,
        orderTag: 'safe-cex',
        price: '62123.5',
        quantity: '0.1000',
        reduceOnly: true,
        visibleQuantity: '0.05',
        marginMode: 'CROSS',
        postOnlyAdjusted: false,
      },
    });

    const signed = prepareWooV3Request(descriptor, auth);
    expect(signed.path).toBe('/v3/trade/order');
    expect(signed.body).toBe(
      '{"symbol":"PERP_BTC_USDT","side":"SELL","positionSide":"LONG","type":"POST_ONLY","clientOrderId":456,"orderTag":"safe-cex","price":"62123.5","quantity":"0.1000","reduceOnly":true,"visibleQuantity":"0.05","marginMode":"CROSS","postOnlyAdjusted":false}',
    );
    expect(signed.signaturePayload).toBe(`${auth.timestamp}POST/v3/trade/order${signed.body}`);
    expect(signed.headers['x-api-signature']).toMatch(/^[a-f0-9]{64}$/);
  });

  it('omits one-way positionSide and proves market orders use base quantity, not spot amount', () => {
    expect(
      buildWooV3RegularPlaceRequest({
        symbol: 'PERP_ETH_USDT',
        side: 'BUY',
        positionMode: 'ONE_WAY',
        type: 'MARKET',
        quantity: '2',
        reduceOnly: false,
      }),
    ).toEqual({
      method: 'POST',
      path: '/v3/trade/order',
      body: {
        symbol: 'PERP_ETH_USDT',
        side: 'BUY',
        type: 'MARKET',
        quantity: '2',
        reduceOnly: false,
        marginMode: 'CROSS',
      },
    });
  });

  it('fails closed on malformed place shapes, unknown enums, and unsafe integer IDs', () => {
    const base = {
      symbol: 'PERP_BTC_USDT',
      side: 'BUY',
      positionMode: 'ONE_WAY',
      type: 'LIMIT',
      quantity: '0.1',
      price: '62000',
      reduceOnly: false,
    } as const;
    expect(() => buildWooV3RegularPlaceRequest({ ...base, symbol: 'BTCUSDT' })).toThrow('PERP_BASE_QUOTE');
    expect(() => buildWooV3RegularPlaceRequest({ ...base, side: 'HOLD' as 'BUY' })).toThrow('BUY, SELL');
    expect(() => buildWooV3RegularPlaceRequest({ ...base, timeInForce: 'DAY' as 'GTC' })).toThrow(
      'GTC, IOC, FOK, POST_ONLY',
    );
    expect(() => buildWooV3RegularPlaceRequest({ ...base, visibleQuantity: '0.2' })).toThrow('must not exceed');
    expect(() =>
      buildWooV3RegularPlaceRequest({ ...base, clientOrderId: Number.MAX_SAFE_INTEGER + 1 }),
    ).toThrow('safe integer');
    expect(() =>
      buildWooV3RegularPlaceRequest({ ...base, postOnlyAdjusted: true }),
    ).toThrow('only valid for POST_ONLY');
  });

  it('strictly decodes place identity, units, echoed fields, and new order ID', () => {
    const request = buildWooV3RegularPlaceRequest({
      symbol: 'PERP_BTC_USDT',
      side: 'BUY',
      positionMode: 'ONE_WAY',
      type: 'LIMIT',
      quantity: '0.1000',
      price: '62000.0',
      reduceOnly: false,
      clientOrderId: 456,
    });
    const fixture = success({
      orderId: 789,
      type: 'LIMIT',
      price: '62000',
      quantity: '0.1',
      amount: '6200',
      clientOrderId: 456,
      bidAskLevel: 0,
    });
    expect(decodeWooV3RegularPlaceResponse(fixture, request, new Set([788]))).toEqual({
      orderId: 789,
      type: 'LIMIT',
      price: '62000',
      quantity: '0.1',
      amount: '6200',
      clientOrderId: 456,
      bidAskLevel: 0,
      timestamp: 1_725_000_000_000,
    });
    expect(() => decodeWooV3RegularPlaceResponse(fixture, request, new Set([789]))).toThrow('reused');
    expect(() =>
      decodeWooV3RegularPlaceResponse(success({ ...fixture.data, clientOrderId: 457 }), request),
    ).toThrow('clientOrderId');
    expect(() => decodeWooV3RegularPlaceResponse(success({ ...fixture.data, type: 'PEGGED' }), request)).toThrow(
      'LIMIT, MARKET, IOC, FOK, POST_ONLY',
    );
  });

  it('builds native amend only with zero-fill provenance and blocks partial-fill ambiguity', () => {
    const ready = buildWooV3RegularAmendRequest({
      currentOrder: currentOrder(),
      target: { orderId: 135 },
      update: { price: '62100', totalQuantity: '0.2' },
    });
    expect(ready).toEqual({
      ready: true,
      request: {
        method: 'PUT',
        path: '/v3/trade/order',
        body: { orderId: 135, price: '62100', quantity: '0.2' },
      },
      target: { orderId: 135 },
      quantitySemantics: 'total-equals-remaining-because-zero-filled-provenance',
    });
    if (!ready.ready) throw new Error('expected ready amend fixture');
    expect(prepareWooV3Request(ready.request, auth).signaturePayload).toBe(
      `${auth.timestamp}PUT/v3/trade/order{"orderId":135,"price":"62100","quantity":"0.2"}`,
    );

    expect(
      buildWooV3RegularAmendRequest({
        currentOrder: currentOrder({ status: 'PARTIAL_FILLED', executed: '0.025' }),
        target: { orderId: 135 },
        update: { totalQuantity: '0.2' },
      }),
    ).toEqual({
      ready: false,
      blocker: 'partial-fill-amend-semantics-require-demo-evidence',
      target: { orderId: 135 },
    });
    expect(
      buildWooV3RegularAmendRequest({
        currentOrder: currentOrder(),
        target: { clientOrderId: 41 },
        update: { price: '62100' },
      }),
    ).toEqual({ ready: false, blocker: 'target-id-mismatch', target: { clientOrderId: 41 } });
  });

  it('places cancel IDs and pending-cancel scope in signed DELETE query strings', () => {
    const cancel = buildWooV3RegularCancelRequest('PERP_BTC_USDT', { orderId: 135 });
    expect(prepareWooV3Request(cancel, auth)).toMatchObject({
      method: 'DELETE',
      path: '/v3/trade/order?orderId=135&symbol=PERP_BTC_USDT',
      signaturePayload: `${auth.timestamp}DELETE/v3/trade/order?orderId=135&symbol=PERP_BTC_USDT`,
    });
    expect(buildWooV3RegularCancelPendingRequest()).toEqual({ method: 'DELETE', path: '/v3/trade/orders' });
    expect(prepareWooV3Request(buildWooV3RegularCancelPendingRequest('PERP_ETH_USDT'), auth).path).toBe(
      '/v3/trade/orders?symbol=PERP_ETH_USDT',
    );
    expect(() =>
      buildWooV3RegularCancelRequest('PERP_BTC_USDT', { orderId: 1, clientOrderId: 2 } as never),
    ).toThrow('exactly one ID');
  });

  it('decodes amend/cancel statuses without claiming response-echoed stable IDs', () => {
    expect(decodeWooV3RegularAmendResponse(success({ status: 'NEW' }), { orderId: 135 })).toEqual({
      status: 'NEW',
      target: { orderId: 135 },
      timestamp: 1_725_000_000_000,
      identityEvidence: 'request-target-only-response-does-not-echo-id',
    });
    expect(decodeWooV3RegularCancelResponse(success({ status: 'CANCELLED' }), { clientOrderId: 42 })).toEqual({
      status: 'CANCELLED',
      target: { clientOrderId: 42 },
      timestamp: 1_725_000_000_000,
      identityEvidence: 'request-target-only-response-does-not-echo-id',
    });
    expect(decodeWooV3RegularCancelPendingResponse(success({ status: 'CANCELLED' }), 'PERP_BTC_USDT')).toEqual({
      status: 'CANCELLED',
      scope: { symbol: 'PERP_BTC_USDT' },
      timestamp: 1_725_000_000_000,
    });
    expect(() => decodeWooV3RegularCancelResponse(success({ status: 'DONE' }), { orderId: 135 })).toThrow(
      'CANCELLED, REJECTED',
    );
  });

  it('preserves documented error evidence and rejects malformed error envelopes', () => {
    try {
      decodeWooV3RegularCancelResponse(
        { success: false, code: 317160, message: 'The order id and symbol are not matched' },
        { orderId: 135 },
      );
      throw new Error('expected gateway error');
    } catch (error) {
      expect(error).toBeInstanceOf(WooV3WriteResponseError);
      expect(error).toMatchObject({
        code: 317160,
        message: 'The order id and symbol are not matched',
        contractStatus: 'vendor-error-envelope-undocumented',
      });
    }
    expect(() =>
      decodeWooV3RegularCancelResponse({ success: false, code: '317160', message: 'bad' }, { orderId: 135 }),
    ).toThrow('safe integer');
    expect(() => decodeWooV3RegularCancelResponse({ success: 'false' }, { orderId: 135 })).toThrow('boolean');
  });

  it('keeps all eight gates false and proves no active Woo runtime imports the write foundation', () => {
    expect(Object.values(WOO_V3_MIGRATION_GATES)).toEqual([false, false, false, false, false, false, false, false]);
    expect(WOO_V3_REGULAR_WRITE_PARITY).toEqual(
      expect.objectContaining({
        runtimeImported: false,
        credentialedDemoLifecycle: false,
        partialFillAmendQuantitySemantics: 'blocked-pending-demo-evidence',
        crossFamilyAllOrders: 'deferred-because-it-also-cancels-algo-orders',
      }),
    );
    const wooDirectory = join(__dirname, '../../exchanges/woo');
    for (const runtimeFile of ['woo.api.ts', 'woo.exchange.ts', 'woo.ws-private.ts', 'woo.ws-public.ts']) {
      expect(readFileSync(join(wooDirectory, runtimeFile), 'utf8')).not.toContain('woo.v3.regular-writes');
    }
    const source = readFileSync(join(wooDirectory, 'woo.v3.regular-writes.ts'), 'utf8');
    expect(source).not.toMatch(/\b(axios|fetch|WebSocket)\b/);
  });
});
