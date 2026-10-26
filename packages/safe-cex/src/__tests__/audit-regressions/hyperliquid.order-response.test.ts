import { describe, expect, it } from 'vitest';

import {
  EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE,
  extractHyperliquidResponseError,
  hyperliquidOrderResponseHasDeferredOrderStatus,
  hyperliquidOrderResponseHasWaitingForTrigger,
  HyperliquidUnreadableOrderResponseError,
  isOnlyHyperliquidTerminalCancelError,
  parseHyperliquidOrderResponse,
} from '../reference/hyperliquid/hyperliquid.order-response';
import { describeHttpFailure } from '../../utils/httpFailure';

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
    expect(hyperliquidOrderResponseHasDeferredOrderStatus(response)).toBe(true);
  });

  it('accepts waitingForFill trigger children without an oid', () => {
    const response = {
      status: 'ok',
      response: {
        type: 'order',
        data: {
          statuses: [{ resting: { oid: 77738308 } }, 'waitingForFill'],
        },
      },
    };

    expect(parseHyperliquidOrderResponse(response)).toEqual({ orderIds: ['77738308'], waitingForTrigger: false });
    expect(hyperliquidOrderResponseHasDeferredOrderStatus(response)).toBe(true);

    const fullyDeferredResponse = {
      status: 'ok',
      response: {
        type: 'order',
        data: {
          statuses: ['waitingForFill'],
        },
      },
    };

    expect(parseHyperliquidOrderResponse(fullyDeferredResponse)).toEqual({ orderIds: [], waitingForTrigger: false });
    expect(hyperliquidOrderResponseHasDeferredOrderStatus(fullyDeferredResponse)).toBe(true);
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

describe('Hyperliquid transport failure reporting', () => {
  // The HOW tunnel adapter resolves non-2xx without axios' settle(), so these
  // arrive as successes. Before this, an unreadable error page reached the user
  // as "Invalid response from Hyperliquid API: null", which never named the
  // status that caused it.
  it('names the status, and never the canonical reason, on a tunnel-delivered non-2xx', () => {
    expect(describeHttpFailure('Hyperliquid', { status: 429, statusText: 'Too Many Requests', data: null })).toBe(
      'Hyperliquid API error (HTTP 429)',
    );
  });

  it('degrades to `HTTP unknown` rather than `HTTP undefined` without a status', () => {
    expect(describeHttpFailure('Hyperliquid', { data: null })).toBe('Hyperliquid API error (HTTP unknown)');
  });

  it("never leaks the tunnel frame's status_text into the message", () => {
    // A real tunnel frame always carries status_text (the Rust side fills it
    // from canonical_reason()), so this is the shape that would regress if the
    // reason were ever put back. `getHttpFailure` still READS it — that is
    // covered in httpFailure.test.ts — it just must not reach the message.
    const message = describeHttpFailure('Hyperliquid', {
      status: 422,
      status_text: 'Unprocessable Entity',
      data: undefined,
    } as never);

    expect(message).toBe('Hyperliquid API error (HTTP 422)');
    expect(message).not.toContain('Unprocessable Entity');
  });

  it('carries the venue body through when there is one', () => {
    expect(
      describeHttpFailure('Hyperliquid', {
        status: 422,
        data: { raw_response: 'Failed to deserialize the JSON body into the target type' },
      }),
    ).toBe(
      'Hyperliquid API error (HTTP 422): {"raw_response":"Failed to deserialize the JSON body into the target type"}',
    );
  });

  it('marks unreadable bodies as unreadable so the caller can reconcile from REST', () => {
    expect(() => parseHyperliquidOrderResponse(null)).toThrow(HyperliquidUnreadableOrderResponseError);
    expect(() => parseHyperliquidOrderResponse({ status: 'ok', response: { type: 'order', data: {} } })).toThrow(
      HyperliquidUnreadableOrderResponseError,
    );
  });

  it('leaves a venue rejection as a plain error, not an unreadable response', () => {
    expect(() =>
      parseHyperliquidOrderResponse({
        status: 'ok',
        response: { type: 'order', data: { statuses: [{ error: 'Order price cannot be more than 80% away' }] } },
      }),
    ).not.toThrow(HyperliquidUnreadableOrderResponseError);
  });

  it('recognizes repeated terminal cancel misses without swallowing mixed failures', () => {
    expect(
      isOnlyHyperliquidTerminalCancelError(
        'Order was never placed, already canceled, or filled. asset=0, Order was never placed, already canceled, or filled. asset=0, Order was never placed, already canceled, or filled. asset=0',
      ),
    ).toBe(true);

    expect(isOnlyHyperliquidTerminalCancelError('Order was never placed, already canceled, or filled.')).toBe(true);

    expect(isOnlyHyperliquidTerminalCancelError('')).toBe(false);
    expect(isOnlyHyperliquidTerminalCancelError(' , . ')).toBe(false);

    expect(
      isOnlyHyperliquidTerminalCancelError(
        'Order was never placed, already canceled, or filled. asset=0, Insufficient margin to place order.',
      ),
    ).toBe(false);
  });

  it('unique-dedupes identical per-order status errors', () => {
    expect(
      extractHyperliquidResponseError({
        status: 'ok',
        response: {
          type: 'cancel',
          data: {
            statuses: [
              { error: 'Order was never placed, already canceled, or filled. asset=0' },
              { error: 'Order was never placed, already canceled, or filled. asset=0' },
              { error: 'Order was never placed, already canceled, or filled. asset=0' },
            ],
          },
        },
      }),
    ).toBe('Order was never placed, already canceled, or filled. asset=0');
  });
});
