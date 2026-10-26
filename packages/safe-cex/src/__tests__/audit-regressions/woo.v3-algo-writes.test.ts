import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  buildWooV3AlgoAmendRequest,
  buildWooV3AlgoCancelPendingRequest,
  buildWooV3AlgoCancelRequest,
  buildWooV3AlgoPlaceRequest,
  decodeWooV3AlgoAmendResponse,
  decodeWooV3AlgoCancelPendingResponse,
  decodeWooV3AlgoCancelResponse,
  decodeWooV3AlgoPlaceResponse,
  WOO_V3_ALGO_WRITE_PARITY,
  WooV3AlgoWriteResponseError,
} from '../reference/woo/woo.v3.algo-writes';
import { WOO_V3_MIGRATION_GATES } from '../reference/woo/woo.v3.contracts';
import type { WooV3AlgoOrder } from '../reference/woo/woo.v3.private';
import { prepareWooV3Request } from '../reference/woo/woo.v3.request';

const auth = {
  apiKey: 'AbmyVJGUpN064ks5ELjLfA==',
  apiSecret: 'QHKRXHPAW1MC9YGZMAT8YDJG2HPR',
  timestamp: 1_578_565_539_808,
} as const;

const currentAlgoOrder = (overrides: Partial<WooV3AlgoOrder> = {}): WooV3AlgoOrder => ({
  algoOrderId: 792_420,
  clientAlgoOrderId: 321,
  rootAlgoOrderId: 792_420,
  parentAlgoOrderId: 0,
  symbol: 'PERP_BTC_USDT',
  algoOrderTag: 'safe-cex',
  algoType: 'STOP',
  side: 'SELL',
  positionSide: 'LONG',
  quantity: '0.1',
  isTriggered: false,
  triggerStatus: 'USELESS',
  type: 'MARKET',
  rootAlgoStatus: 'NEW',
  algoStatus: 'NEW',
  triggerPriceType: 'MARKET_PRICE',
  triggerTime: '0',
  triggerPrice: '60000',
  totalExecutedQuantity: '0',
  averageExecutedPrice: '0',
  totalFee: '0',
  feeAsset: '',
  totalRebate: 0,
  rebateCurrency: 'USDT',
  reduceOnly: true,
  createdTime: 1_696_595_230_885,
  updatedTime: 1_696_595_230_885,
  isActivated: false,
  childOrders: [],
  ...overrides,
});

const success = (data: Record<string, unknown>) => ({ success: true, timestamp: 1_725_000_000_000, data });

describe('WooX V3 inert algo-order writes', () => {
  it('builds and signs an exact zero-fill STOP market close using the documented last-price trigger', () => {
    const built = buildWooV3AlgoPlaceRequest({
      family: 'STOP_MARKET',
      symbol: 'PERP_BTC_USDT',
      side: 'SELL',
      positionMode: 'HEDGE_MODE',
      quantity: '0.1000',
      triggerPrice: '60000',
      triggerPriceType: 'MARKET_PRICE',
      clientAlgoOrderId: 321,
      algoOrderTag: 'safe-cex',
    });
    expect(built).toEqual({
      ready: true,
      request: {
        method: 'POST',
        path: '/v3/trade/algoOrder',
        body: {
          symbol: 'PERP_BTC_USDT',
          side: 'SELL',
          positionSide: 'LONG',
          clientAlgoOrderId: 321,
          algoOrderTag: 'safe-cex',
          algoType: 'STOP',
          quantity: '0.1000',
          marginMode: 'CROSS',
          triggerPrice: '60000',
          triggerPriceType: 'MARKET_PRICE',
          type: 'MARKET',
          reduceOnly: true,
        },
      },
      family: 'STOP_MARKET',
      triggerSemantics: 'official-last-price-trigger',
    });
    if (!built.ready) throw new Error('expected ready STOP fixture');
    const prepared = prepareWooV3Request(built.request, auth);
    expect(prepared.signaturePayload).toBe(`${auth.timestamp}POST/v3/trade/algoOrder${prepared.body}`);
    expect(prepared.body).toBe(
      '{"symbol":"PERP_BTC_USDT","side":"SELL","positionSide":"LONG","clientAlgoOrderId":321,"algoOrderTag":"safe-cex","algoType":"STOP","quantity":"0.1000","marginMode":"CROSS","triggerPrice":"60000","triggerPriceType":"MARKET_PRICE","type":"MARKET","reduceOnly":true}',
    );
  });

  it('blocks undocumented trigger types instead of translating safe-cex mark/index labels', () => {
    expect(
      buildWooV3AlgoPlaceRequest({
        family: 'STOP_MARKET',
        symbol: 'PERP_BTC_USDT',
        side: 'SELL',
        positionMode: 'ONE_WAY',
        quantity: '0.1',
        triggerPrice: '60000',
        triggerPriceType: 'MARK_PRICE',
      }),
    ).toEqual({ ready: false, blocker: 'only-market-price-trigger-is-documented', family: 'STOP_MARKET' });
  });

  it('builds a vendor-native trailing callback without inventing conversion units', () => {
    const built = buildWooV3AlgoPlaceRequest({
      family: 'TRAILING_STOP',
      symbol: 'PERP_ETH_USDT',
      side: 'BUY',
      positionMode: 'ONE_WAY',
      quantity: '2',
      callback: { source: 'WOO_NATIVE_CALLBACK_VALUE', value: '25.5' },
      activatedPrice: '3000',
    });
    expect(built).toEqual({
      ready: true,
      request: {
        method: 'POST',
        path: '/v3/trade/algoOrder',
        body: {
          symbol: 'PERP_ETH_USDT',
          side: 'BUY',
          positionSide: 'BOTH',
          algoType: 'TRAILING_STOP',
          quantity: '2',
          marginMode: 'CROSS',
          type: 'MARKET',
          callbackValue: '25.5',
          activatedPrice: '3000',
          reduceOnly: true,
        },
      },
      family: 'TRAILING_STOP',
      triggerSemantics: 'vendor-native-callback-value-no-safe-cex-conversion',
    });
    expect(() =>
      buildWooV3AlgoPlaceRequest({
        family: 'TRAILING_STOP',
        symbol: 'PERP_ETH_USDT',
        side: 'BUY',
        positionMode: 'ONE_WAY',
        quantity: '2',
        callback: { source: 'SAFE_CEX_ABSOLUTE_TRIGGER' as 'WOO_NATIVE_CALLBACK_VALUE', value: '25.5' },
      }),
    ).toThrow('WOO_NATIVE_CALLBACK_VALUE');
  });

  it('builds exact positional TP/SL children and caps the family to one child of each kind', () => {
    const built = buildWooV3AlgoPlaceRequest({
      family: 'POSITIONAL_TP_SL',
      symbol: 'PERP_BTC_USDT',
      side: 'SELL',
      positionMode: 'HEDGE_MODE',
      takeProfit: { triggerPrice: '70000', triggerPriceType: 'MARKET_PRICE' },
      stopLoss: { triggerPrice: '60000', triggerPriceType: 'MARKET_PRICE' },
    });
    expect(built).toEqual({
      ready: true,
      request: {
        method: 'POST',
        path: '/v3/trade/algoOrder',
        body: {
          symbol: 'PERP_BTC_USDT',
          algoType: 'POSITIONAL_TP_SL',
          childOrders: [
            {
              algoType: 'TAKE_PROFIT',
              side: 'SELL',
              triggerPrice: '70000',
              type: 'CLOSE_POSITION',
              triggerPriceType: 'MARKET_PRICE',
            },
            {
              algoType: 'STOP_LOSS',
              side: 'SELL',
              triggerPrice: '60000',
              type: 'CLOSE_POSITION',
              triggerPriceType: 'MARKET_PRICE',
            },
          ],
          positionSide: 'LONG',
          marginMode: 'CROSS',
        },
      },
      family: 'POSITIONAL_TP_SL',
      triggerSemantics: 'official-last-price-positional-children',
    });
    expect(() =>
      buildWooV3AlgoPlaceRequest({
        family: 'POSITIONAL_TP_SL',
        symbol: 'PERP_BTC_USDT',
        side: 'SELL',
        positionMode: 'HEDGE_MODE',
      }),
    ).toThrow('at least one child');
  });

  it('strictly decodes single-order identity and blocks positional parent/child identity inference', () => {
    const stop = buildWooV3AlgoPlaceRequest({
      family: 'STOP_MARKET',
      symbol: 'PERP_BTC_USDT',
      side: 'SELL',
      positionMode: 'HEDGE_MODE',
      quantity: '0.1',
      triggerPrice: '60000',
      triggerPriceType: 'MARKET_PRICE',
      clientAlgoOrderId: 321,
    });
    if (!stop.ready) throw new Error('expected ready STOP fixture');
    const fixture = success({
      rows: [{ algoType: 'STOP', clientAlgoOrderId: 321, algoOrderId: 792_420, quantity: '0.1000' }],
    });
    expect(decodeWooV3AlgoPlaceResponse(fixture, stop, new Set([792_419]))).toEqual({
      ready: true,
      row: { algoType: 'STOP', clientAlgoOrderId: 321, algoOrderId: 792_420, quantity: '0.1000' },
      timestamp: 1_725_000_000_000,
      identityEvidence: 'response-echoed-algo-order-id',
    });
    expect(() => decodeWooV3AlgoPlaceResponse(fixture, stop, new Set([792_420]))).toThrow('reused');

    const positional = buildWooV3AlgoPlaceRequest({
      family: 'POSITIONAL_TP_SL',
      symbol: 'PERP_BTC_USDT',
      side: 'SELL',
      positionMode: 'HEDGE_MODE',
      stopLoss: { triggerPrice: '60000', triggerPriceType: 'MARKET_PRICE' },
    });
    if (!positional.ready) throw new Error('expected ready positional fixture');
    expect(
      decodeWooV3AlgoPlaceResponse(
        success({
          rows: [
            { algoOrderId: 432_133, clientAlgoOrderId: 0, algoType: 'STOP_LOSS', quantity: '0' },
            { algoOrderId: 432_131, clientAlgoOrderId: 0, algoType: 'POSITIONAL_TP_SL', quantity: '0' },
          ],
        }),
        positional,
      ),
    ).toMatchObject({
      ready: false,
      blocker: 'positional-response-parent-child-identity-requires-demo-evidence',
      timestamp: 1_725_000_000_000,
    });
  });

  it('builds only zero-fill untriggered STOP trigger edits and signs the exact JSON body', () => {
    const result = buildWooV3AlgoAmendRequest({
      currentOrder: currentAlgoOrder(),
      target: { algoOrderId: 792_420 },
      update: { triggerPrice: '59000', triggerPriceType: 'MARKET_PRICE' },
    });
    expect(result).toEqual({
      ready: true,
      request: {
        method: 'PUT',
        path: '/v3/trade/algoOrder',
        body: { algoOrderId: 792_420, triggerPrice: '59000', triggerPriceType: 'MARKET_PRICE' },
      },
      target: { algoOrderId: 792_420 },
      updateSemantics: 'untriggered-zero-fill-stop-trigger-only',
    });
    if (!result.ready) throw new Error('expected ready amend fixture');
    expect(prepareWooV3Request(result.request, auth).signaturePayload).toBe(
      `${auth.timestamp}PUT/v3/trade/algoOrder{"algoOrderId":792420,"triggerPrice":"59000","triggerPriceType":"MARKET_PRICE"}`,
    );
  });

  it('returns typed blockers for quantity, partial-fill, positional, and trailing amend ambiguity', () => {
    expect(
      buildWooV3AlgoAmendRequest({
        currentOrder: currentAlgoOrder(),
        target: { algoOrderId: 792_420 },
        update: { quantity: '0.2' },
      }),
    ).toMatchObject({ ready: false, blocker: 'quantity-amend-semantics-require-demo-evidence' });
    expect(
      buildWooV3AlgoAmendRequest({
        currentOrder: currentAlgoOrder({ algoStatus: 'PARTIAL_FILLED', totalExecutedQuantity: '0.01' }),
        target: { algoOrderId: 792_420 },
        update: { triggerPrice: '59000' },
      }),
    ).toMatchObject({ ready: false, blocker: 'partial-fill-algo-amend-semantics-require-demo-evidence' });
    expect(
      buildWooV3AlgoAmendRequest({
        currentOrder: currentAlgoOrder({ algoType: 'POSITIONAL_TP_SL' }),
        target: { algoOrderId: 792_420 },
        update: { triggerPrice: '59000' },
      }),
    ).toMatchObject({ ready: false, blocker: 'positional-child-update-not-documented' });
    expect(
      buildWooV3AlgoAmendRequest({
        currentOrder: currentAlgoOrder({ algoType: 'TRAILING_STOP' }),
        target: { algoOrderId: 792_420 },
        update: { callbackValue: '30' },
      }),
    ).toMatchObject({ ready: false, blocker: 'trailing-callback-update-not-documented' });
    expect(
      buildWooV3AlgoAmendRequest({
        currentOrder: currentAlgoOrder({ triggerPriceType: 'INDEX_PRICE' }),
        target: { algoOrderId: 792_420 },
        update: { triggerPrice: '59000' },
      }),
    ).toMatchObject({ ready: false, blocker: 'only-market-price-trigger-is-documented' });
    expect(
      buildWooV3AlgoAmendRequest({
        currentOrder: currentAlgoOrder({ triggerPrice: undefined }),
        target: { algoOrderId: 792_420 },
        update: { triggerPrice: '59000' },
      }),
    ).toMatchObject({ ready: false, blocker: 'stop-trigger-provenance-incomplete' });
  });

  it('places cancel IDs in DELETE query and blocks the contradictory symbol pending-cancel scope', () => {
    const cancel = buildWooV3AlgoCancelRequest({ clientAlgoOrderId: 321 });
    expect(prepareWooV3Request(cancel, auth)).toMatchObject({
      method: 'DELETE',
      path: '/v3/trade/algoOrder?clientAlgoOrderId=321',
      signaturePayload: `${auth.timestamp}DELETE/v3/trade/algoOrder?clientAlgoOrderId=321`,
    });
    expect(buildWooV3AlgoCancelPendingRequest()).toEqual({
      ready: true,
      request: { method: 'DELETE', path: '/v3/trade/algoOrders' },
      scope: 'all-algo-orders-all-symbols',
    });
    expect(buildWooV3AlgoCancelPendingRequest({ symbol: 'PERP_BTC_USDT' })).toEqual({
      ready: false,
      blocker: 'symbol-scoped-pending-cancel-query-schema-conflicts-with-description',
      scope: { symbol: 'PERP_BTC_USDT' },
    });
  });

  it('decodes current empty mutation envelopes without fabricating status or response IDs', () => {
    expect(decodeWooV3AlgoAmendResponse(success({}), { algoOrderId: 792_420 })).toEqual({
      accepted: true,
      target: { algoOrderId: 792_420 },
      timestamp: 1_725_000_000_000,
      identityEvidence: 'request-target-only-response-does-not-echo-id-or-status',
      gatewayData: {},
    });
    expect(decodeWooV3AlgoCancelResponse(success({}), { clientAlgoOrderId: 321 })).toEqual({
      accepted: true,
      target: { clientAlgoOrderId: 321 },
      timestamp: 1_725_000_000_000,
      identityEvidence: 'request-target-only-response-does-not-echo-id-or-status',
      gatewayData: {},
    });
    expect(decodeWooV3AlgoCancelPendingResponse(success({}))).toEqual({
      accepted: true,
      scope: 'all-algo-orders-all-symbols',
      timestamp: 1_725_000_000_000,
      gatewayData: {},
    });
  });

  it('preserves vendor error evidence and rejects malformed envelopes', () => {
    try {
      decodeWooV3AlgoCancelResponse(
        { success: false, code: 317_160, message: 'The order id is invalid' },
        { algoOrderId: 792_420 },
      );
      throw new Error('expected vendor error');
    } catch (error) {
      expect(error).toBeInstanceOf(WooV3AlgoWriteResponseError);
      expect(error).toMatchObject({
        code: 317_160,
        message: 'The order id is invalid',
        contractStatus: 'vendor-error-envelope-undocumented',
      });
    }
    expect(() => decodeWooV3AlgoCancelResponse({ success: 'true' }, { algoOrderId: 1 })).toThrow('boolean');
  });

  it('keeps every migration gate false and proves no active runtime import or transport exists', () => {
    expect(Object.values(WOO_V3_MIGRATION_GATES)).toEqual([false, false, false, false, false, false, false, false]);
    expect(WOO_V3_ALGO_WRITE_PARITY).toEqual(
      expect.objectContaining({
        runtimeImported: false,
        credentialedDemoLifecycle: false,
        positionalResponseIdentity: 'blocked-pending-demo-evidence',
        positionalUpdate: 'blocked-official-edit-schema-has-no-childOrders',
        symbolPendingCancel: 'blocked-published-query-schema-conflicts-with-prose',
      }),
    );
    const wooDirectory = join(__dirname, '../../exchanges/woo');
    for (const runtimeFile of ['woo.api.ts', 'woo.exchange.ts', 'woo.ws-private.ts', 'woo.ws-public.ts']) {
      expect(readFileSync(join(wooDirectory, runtimeFile), 'utf8')).not.toContain('woo.v3.algo-writes');
    }
    const source = readFileSync(join(wooDirectory, 'woo.v3.algo-writes.ts'), 'utf8');
    expect(source).not.toMatch(/\b(axios|fetch|WebSocket)\b/);
  });
});
