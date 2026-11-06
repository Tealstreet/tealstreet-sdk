import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it, vi } from 'vitest';

import { OrderTimeInForce } from '../../types';
import {
  CoinbaseStarbaseJsonRpcClient,
  CoinbaseStarbaseProtocolError,
  CoinbaseStarbaseRpcError,
} from '../reference/coinbase/coinbase.starbase';
import {
  assessCoinbaseStarbaseWriteResultParity,
  buildCoinbaseStarbaseBuy,
  buildCoinbaseStarbaseCancel,
  buildCoinbaseStarbaseClosePosition,
  buildCoinbaseStarbaseEdit,
  buildCoinbaseStarbasePlaceOrder,
  buildCoinbaseStarbaseSell,
  coinbaseStarbaseTotalBaseAmount,
  coinbaseStarbaseTotalContracts,
  coinbaseStarbaseUnresolvedValidUntil,
  decodeCoinbaseStarbaseCancelResult,
  decodeCoinbaseStarbaseWriteResult,
  mapCoinbaseStarbaseTimeInForce,
} from '../reference/coinbase/coinbase.starbase-orders';
import {
  STARBASE_FILLED_WRITE_RESULT_FIXTURE,
  STARBASE_OTOCO_CHILD_FIXTURE,
  STARBASE_TRIGGERED_ORDER_FIXTURE,
  STARBASE_WRITE_ERROR_RESPONSE,
  STARBASE_WRITE_ORDER_FIXTURE,
  STARBASE_WRITE_RESULT_FIXTURE,
} from './coinbase.starbase-orders.fixtures';

describe('Coinbase Starbase inert order-write contracts', () => {
  it('builds exact buy and sell payloads from explicit total base-amount intent', () => {
    const buy = buildCoinbaseStarbaseBuy({
      label: 'display-only-label',
      price: 65_000.5,
      savedSymbol: 'BTC-PERP-INTX',
      size: coinbaseStarbaseTotalBaseAmount(0.001),
      timeInForce: OrderTimeInForce.PostOnly,
      type: 'limit',
    });
    expect(buy).toEqual({
      status: 'ready',
      value: {
        method: 'private/buy',
        params: {
          amount: 0.001,
          instrument_name: 'BTC_USDC-PERPETUAL',
          label: 'display-only-label',
          post_only: true,
          price: 65_000.5,
          time_in_force: 'good_til_cancelled',
          type: 'limit',
        },
      },
    });

    expect(
      buildCoinbaseStarbaseSell({
        price: 49_000,
        savedSymbol: 'BTC-PERP-INTX',
        size: coinbaseStarbaseTotalBaseAmount(0.1),
        timeInForce: OrderTimeInForce.GoodTillCancel,
        trigger: 'mark_price',
        triggerPrice: 49_500,
        type: 'stop_limit',
      }),
    ).toEqual({
      status: 'ready',
      value: {
        method: 'private/sell',
        params: {
          amount: 0.1,
          instrument_name: 'BTC_USDC-PERPETUAL',
          post_only: false,
          price: 49_000,
          time_in_force: 'good_til_cancelled',
          trigger: 'mark_price',
          trigger_price: 49_500,
          type: 'stop_limit',
        },
      },
    });
  });

  it('maps safe-cex TIF values to the published lowercase fields', () => {
    expect(mapCoinbaseStarbaseTimeInForce(OrderTimeInForce.GoodTillCancel)).toEqual({
      post_only: false,
      time_in_force: 'good_til_cancelled',
    });
    expect(mapCoinbaseStarbaseTimeInForce(OrderTimeInForce.ImmediateOrCancel)).toEqual({
      post_only: false,
      time_in_force: 'immediate_or_cancel',
    });
    expect(mapCoinbaseStarbaseTimeInForce(OrderTimeInForce.FillOrKill)).toEqual({
      post_only: false,
      time_in_force: 'fill_or_kill',
    });
    expect(mapCoinbaseStarbaseTimeInForce(OrderTimeInForce.PostOnly)).toEqual({
      post_only: true,
      time_in_force: 'good_til_cancelled',
    });
  });

  it('blocks contracts and size edits without inventing conversions or partial-order semantics', () => {
    const contractIntent = {
      savedSymbol: 'BTC-PERP-INTX',
      side: 'buy' as const,
      size: coinbaseStarbaseTotalContracts(10),
      type: 'market' as const,
    };
    expect(buildCoinbaseStarbasePlaceOrder(contractIntent)).toEqual({
      status: 'blocked',
      blocker: 'contract-unit-conversion-unverified',
      intent: contractIntent,
    });

    const editIntent = {
      orderId: 'USDC-ORDER-1',
      size: coinbaseStarbaseTotalBaseAmount(0.2),
    };
    expect(buildCoinbaseStarbaseEdit(editIntent)).toEqual({
      status: 'blocked',
      blocker: 'edit-total-vs-remaining-amount-unresolved',
      intent: editIntent,
    });
  });

  it('blocks market-limit and valid_until where the official response shape or timestamp unit is absent', () => {
    expect(
      buildCoinbaseStarbaseBuy({
        savedSymbol: 'BTC-PERP-INTX',
        size: coinbaseStarbaseTotalBaseAmount(0.1),
        type: 'market_limit',
      }),
    ).toMatchObject({ blocker: 'market-limit-response-schema-missing', status: 'blocked' });

    const validUntil = coinbaseStarbaseUnresolvedValidUntil(1_800_000_000_000);
    expect(
      buildCoinbaseStarbaseBuy({
        savedSymbol: 'BTC-PERP-INTX',
        size: coinbaseStarbaseTotalBaseAmount(0.1),
        type: 'market',
        validUntil,
      }),
    ).toMatchObject({ blocker: 'valid-until-unit-unpublished', status: 'blocked' });
    expect(() => coinbaseStarbaseUnresolvedValidUntil(1.5)).toThrow('expected a safe integer');
  });

  it('builds the published OTOCO entry and two explicit reduce-only exit legs', () => {
    expect(() =>
      buildCoinbaseStarbaseSell({
        price: 65_000,
        savedSymbol: 'BTC-PERP-INTX',
        size: coinbaseStarbaseTotalBaseAmount(0.1),
        trigger: 'last_price',
        type: 'take_limit',
      }),
    ).toThrow('take_limit requires trigger_price');

    expect(
      buildCoinbaseStarbasePlaceOrder({
        otoco: {
          legs: [
            {
              direction: 'sell',
              price: 65_000,
              reduceOnly: true,
              size: coinbaseStarbaseTotalBaseAmount(0.1),
              trigger: 'last_price',
              type: 'take_limit',
            },
            {
              direction: 'sell',
              reduceOnly: true,
              size: coinbaseStarbaseTotalBaseAmount(0.1),
              trigger: 'mark_price',
              triggerPrice: 55_000,
              type: 'stop_market',
            },
          ],
          linkedOrderType: 'one_triggers_one_cancels_other',
        },
        price: 60_000,
        savedSymbol: 'BTC-PERP-INTX',
        side: 'buy',
        size: coinbaseStarbaseTotalBaseAmount(0.1),
        type: 'limit',
      }),
    ).toEqual({
      status: 'ready',
      value: {
        method: 'private/buy',
        params: {
          amount: 0.1,
          instrument_name: 'BTC_USDC-PERPETUAL',
          linked_order_type: 'one_triggers_one_cancels_other',
          otoco_config: [
            {
              amount: 0.1,
              direction: 'sell',
              price: 65_000,
              reduce_only: true,
              trigger: 'last_price',
              type: 'take_limit',
            },
            {
              amount: 0.1,
              direction: 'sell',
              reduce_only: true,
              trigger: 'mark_price',
              trigger_price: 55_000,
              type: 'stop_market',
            },
          ],
          price: 60_000,
          type: 'limit',
        },
      },
    });
  });

  it('uses exchange order IDs for edit/cancel and never exposes label-addressed writes', () => {
    expect(buildCoinbaseStarbaseEdit({ orderId: 'USDC-ORDER-1', price: 50_100 })).toEqual({
      status: 'ready',
      value: {
        method: 'private/edit',
        params: { order_id: 'USDC-ORDER-1', price: 50_100 },
      },
    });
    expect(buildCoinbaseStarbaseCancel('USDC-ORDER-1')).toEqual({
      method: 'private/cancel',
      params: { order_id: 'USDC-ORDER-1' },
    });
    const source = readFileSync(resolve(__dirname, '../reference/coinbase/coinbase.starbase-orders.ts'), 'utf8');
    expect(source).not.toContain('private/edit_by_label');
    expect(source).not.toContain('private/cancel_by_label');
  });

  it('builds exact market and limit close-position requests', () => {
    expect(
      buildCoinbaseStarbaseClosePosition({
        savedSymbol: 'BTC-PERP-INTX',
        type: 'market',
      }),
    ).toEqual({
      method: 'private/close_position',
      params: { instrument_name: 'BTC_USDC-PERPETUAL', type: 'market' },
    });
    expect(
      buildCoinbaseStarbaseClosePosition({
        price: 49_500,
        savedSymbol: 'BTC-PERP-INTX',
        type: 'limit',
      }),
    ).toEqual({
      method: 'private/close_position',
      params: {
        instrument_name: 'BTC_USDC-PERPETUAL',
        price: 49_500,
        type: 'limit',
      },
    });
  });

  it('models canonical IDs, stop-trigger transitions, and OTOCO parent/child IDs without labels', () => {
    const primary = decodeCoinbaseStarbaseWriteResult(STARBASE_WRITE_RESULT_FIXTURE);
    expect(primary.identity).toEqual({
      canonicalOrderId: 'USDC-PRIMARY-1',
      nonCanonicalLabel: 'display-label-is-not-unique',
      otocoChildOrderIds: ['USDC-TP-1', 'USDC-SL-1'],
    });

    const triggered = decodeCoinbaseStarbaseCancelResult(STARBASE_TRIGGERED_ORDER_FIXTURE);
    expect(triggered.identity).toEqual({
      canonicalOrderId: 'USDC-POST-TRIGGER-1',
      nonCanonicalLabel: 'display-label-is-not-unique',
      otocoChildOrderIds: [],
      preTriggerOrderId: 'USDC-PRE-TRIGGER-1',
    });

    const child = decodeCoinbaseStarbaseCancelResult(STARBASE_OTOCO_CHILD_FIXTURE);
    expect(child.identity).toEqual({
      canonicalOrderId: 'USDC-TP-1',
      nonCanonicalLabel: 'display-label-is-not-unique',
      otocoChildOrderIds: [],
      otocoPrimaryOrderId: 'USDC-PRIMARY-1',
    });
    expect(
      new Set([primary.identity.canonicalOrderId, triggered.identity.canonicalOrderId, child.identity.canonicalOrderId])
        .size,
    ).toBe(3);
  });

  it('gates filled write responses whose inherited OpenAPI amount units conflict with the guide', () => {
    expect(
      assessCoinbaseStarbaseWriteResultParity(decodeCoinbaseStarbaseWriteResult(STARBASE_WRITE_RESULT_FIXTURE)),
    ).toMatchObject({
      status: 'ready',
    });
    expect(
      assessCoinbaseStarbaseWriteResultParity(decodeCoinbaseStarbaseWriteResult(STARBASE_FILLED_WRITE_RESULT_FIXTURE)),
    ).toMatchObject({
      blocker: 'response-trade-amount-unit-conflict',
      status: 'blocked',
    });
  });

  it('rejects malformed results and preserves canonical JSON-RPC errors', async () => {
    expect(() =>
      decodeCoinbaseStarbaseWriteResult({
        order: STARBASE_WRITE_ORDER_FIXTURE,
        trades: {},
      }),
    ).toThrow(CoinbaseStarbaseProtocolError);
    expect(() =>
      decodeCoinbaseStarbaseCancelResult({
        ...STARBASE_WRITE_ORDER_FIXTURE,
        oto_order_ids: ['same', 'same'],
      }),
    ).toThrow('Duplicate Starbase order oto_order_ids');

    const client = new CoinbaseStarbaseJsonRpcClient(vi.fn().mockResolvedValue(STARBASE_WRITE_ERROR_RESPONSE), {
      kind: 'http',
    });
    await expect(client.request('private/cancel', { order_id: 'missing-id' })).rejects.toMatchObject({
      code: 10004,
      message: 'order_not_found',
      method: 'private/cancel',
      name: 'CoinbaseStarbaseRpcError',
      requestId: 1,
    } satisfies Partial<CoinbaseStarbaseRpcError>);
  });

  it('remains statically inert outside the Starbase contract files', () => {
    for (const file of [
      '__tests__/reference/coinbase/coinbase.api.ts',
      '__tests__/reference/coinbase/coinbase.exchange.ts',
      '__tests__/reference/coinbase/coinbase.ws-private.ts',
      '__tests__/reference/coinbase/coinbase.ws-public.ts',
      'createExchangeFromAccount.ts',
      'exchangeMetadata.ts',
      'transport/tealstreet/routing/routingTable.ts',
    ]) {
      const source = readFileSync(resolve(__dirname, '../..', file), 'utf8');
      expect(source).not.toContain('coinbase.starbase-orders');
    }
  });
});
