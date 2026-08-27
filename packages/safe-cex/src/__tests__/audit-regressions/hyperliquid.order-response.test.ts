import { describe, expect, it } from 'vitest';

import {
  EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE,
  extractHyperliquidResponseError,
  hyperliquidOrderResponseHasWaitingForTrigger,
  parseHyperliquidOrderResponse,
} from '../../exchanges/hyperliquid/hyperliquid.order-response';

describe('Hyperliquid order response contract', () => {
  it('extracts documented resting and filled order ids', () => {
    expect(
      parseHyperliquidOrderResponse({
        status: 'ok',
        response: {
          type: 'order',
          data: {
            statuses: [{ resting: { oid: 77738308 } }, { filled: { totalSz: '0.02', avgPx: '1891.4', oid: 77747314 } }],
          },
        },
      }),
    ).toEqual({ orderIds: ['77738308', '77747314'], waitingForTrigger: false });
  });

  it('accepts CCXT-observed waitingForTrigger statuses without an oid', () => {
    const response = {
      status: 'ok',
      response: {
        type: 'order',
        data: {
          statuses: ['waitingForTrigger'],
        },
      },
    };

    expect(parseHyperliquidOrderResponse(response)).toEqual({ orderIds: [], waitingForTrigger: true });
    expect(hyperliquidOrderResponseHasWaitingForTrigger(response)).toBe(true);
  });

  it('surfaces documented per-order errors from the statuses vector', () => {
    const response = {
      status: 'ok',
      response: {
        type: 'order',
        data: {
          statuses: [{ error: 'Invalid TP/SL price.' }, { error: 'Order must have minimum value of $10.' }],
        },
      },
    };

    expect(extractHyperliquidResponseError(response)).toBe(
      'Invalid TP/SL price., Order must have minimum value of $10.',
    );
    expect(() => parseHyperliquidOrderResponse(response)).toThrow(
      'Invalid TP/SL price., Order must have minimum value of $10.',
    );
  });

  it('surfaces whole-action pre-validation errors before looking for statuses', () => {
    const response = { status: 'err', response: 'Non-reduce-only TP/SL orders are not allowed.' };

    expect(extractHyperliquidResponseError(response)).toBe('Non-reduce-only TP/SL orders are not allowed.');
    expect(() => parseHyperliquidOrderResponse(response)).toThrow('Non-reduce-only TP/SL orders are not allowed.');
  });

  it('keeps empty order response bodies distinct from schema drift', () => {
    expect(() => parseHyperliquidOrderResponse(null)).toThrow(EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE);
  });

  it('fails closed on unknown successful status shapes', () => {
    expect(() =>
      parseHyperliquidOrderResponse({
        status: 'ok',
        response: {
          type: 'order',
          data: {
            statuses: ['newStatusFromVenue'],
          },
        },
      }),
    ).toThrow('Hyperliquid order response contained unsupported status: "newStatusFromVenue"');
  });
});
