import { describe, expect, it } from 'vitest';

import {
  deserializeCommandError,
  exchangeErrorCategory,
  exchangeErrorTags,
  isErrorReported,
  markErrorReported,
  serializeCommandError,
} from './commandOutcome';

// The worker and normal CLI share this exact allowlist at their clone boundary.
describe('command outcome serialization', () => {
  it('preserves reporting state through the shared worker and CLI allowlist', () => {
    const original = new Error('reported');
    markErrorReported(original);
    expect(original).toHaveProperty('errorReported', true);
    expect(isErrorReported(original)).toBe(true);
    const restored = deserializeCommandError(structuredClone(serializeCommandError(original)));
    expect(restored).toHaveProperty('errorReported', true);
    expect(isErrorReported(restored)).toBe(true);
  });

  it('records frozen-error reporting without replacing or mutating its command outcome', () => {
    const original = Object.freeze(
      Object.assign(new Error('interrupted'), {
        outcome: 'unknown',
        clientOrderIds: ['requested'],
        actualClientOrderIds: ['actual'],
        partialOrderIds: ['accepted'],
      }),
    );
    markErrorReported(original);
    expect(original).not.toHaveProperty('errorReported');
    expect(isErrorReported(original)).toBe(true);
    expect(serializeCommandError(original)).toMatchObject({
      errorReported: true,
      outcome: 'unknown',
      clientOrderIds: ['requested'],
      actualClientOrderIds: ['actual'],
      partialOrderIds: ['accepted'],
    });
    expect(isErrorReported(new Error('interrupted'))).toBe(false);
    expect(serializeCommandError({ message: 'unreported', errorReported: 'true' })).toEqual({ message: 'unreported' });
    expect(serializeCommandError({ message: 'unreported', errorReported: false })).toEqual({
      message: 'unreported',
      errorReported: false,
    });
  });

  it('keeps primitive failures out of the reporting ledger', () => {
    for (const value of ['same text', undefined, null, 1]) {
      markErrorReported(value);
      expect(isErrorReported(value)).toBe(false);
    }
  });
  it.each(['ENGINE_DEAD', 'COMMAND_TIMEOUT', 'ENGINE_DISPOSED'])(
    'round trips %s attribution through structured clone',
    (code) => {
      const error = Object.assign(new Error('Write interrupted'), {
        name: 'InterruptedCommandError',
        code,
        command: 'placeOrders',
        timeoutMs: 120000,
        outcome: 'unknown',
        clientOrderId: 'caller-a',
        clientOrderIds: ['caller-a', 'caller-b'],
        request: { secret: 'must not cross' },
        cause: new Error('internal'),
      });
      const details = serializeCommandError(error);
      const restored = deserializeCommandError(structuredClone(details));
      expect(restored).toBeInstanceOf(Error);
      expect(restored).toMatchObject({
        message: 'Write interrupted',
        name: 'InterruptedCommandError',
        code,
        command: 'placeOrders',
        timeoutMs: 120000,
        outcome: 'unknown',
        clientOrderId: 'caller-a',
        clientOrderIds: ['caller-a', 'caller-b'],
      });
      expect(restored).not.toHaveProperty('request');
      expect(restored).not.toHaveProperty('cause');
      expect(details.clientOrderIds).not.toBe(error.clientOrderIds);
    },
  );

  it('retains known partial failure receipts without inventing an unknown outcome', () => {
    const error = Object.assign(new Error('Second order refused'), {
      code: 400,
      command: 'placeOrders',
      partialOrderIds: ['venue-accepted'],
      partialOrderCount: 1,
      totalOrderCount: 2,
      failedOrderIndex: 1,
    });
    const restored = deserializeCommandError(structuredClone(serializeCommandError(error)));
    expect(restored).toMatchObject({
      message: error.message,
      code: 400,
      command: 'placeOrders',
      partialOrderIds: ['venue-accepted'],
      partialOrderCount: 1,
      totalOrderCount: 2,
      failedOrderIndex: 1,
    });
    expect(restored).not.toHaveProperty('outcome');
    expect(restored).not.toHaveProperty('clientOrderId');
    expect(restored.partialOrderIds).not.toBe(error.partialOrderIds);
  });

  it('copies actual receipts without conflating requested IDs or numeric identity namespaces', () => {
    const error = Object.assign(new Error('Interrupted'), {
      receiptKey: 'host-session:7',
      commandId: 7,
      clientOrderId: 'requested-parent',
      clientOrderIds: ['requested-parent'],
      actualClientOrderIds: ['selected-parent', 'generated-child'],
      orderIdentities: [
        { symbol: 'BTCUSDT', kind: 'clientOrderId', identity: 'selected-parent', raw: { secret: 'excluded' } },
        { symbol: 'BTCUSDT', kind: 'clientOrderId', identity: 'generated-child' },
        { symbol: 'ETHUSDT', kind: 'clientOrderIndex', identity: '9007199254740993' },
        { symbol: 'BTCUSDT', kind: 'orderCode', identity: '00000000000000000017' },
      ],
    });
    const details = serializeCommandError(error);
    const restored = deserializeCommandError(structuredClone(details));
    expect(restored).toMatchObject({
      receiptKey: 'host-session:7',
      commandId: 7,
      clientOrderId: 'requested-parent',
      clientOrderIds: ['requested-parent'],
      actualClientOrderIds: ['selected-parent', 'generated-child'],
      orderIdentities: [
        { symbol: 'BTCUSDT', kind: 'clientOrderId', identity: 'selected-parent' },
        { symbol: 'BTCUSDT', kind: 'clientOrderId', identity: 'generated-child' },
        { symbol: 'ETHUSDT', kind: 'clientOrderIndex', identity: '9007199254740993' },
        { symbol: 'BTCUSDT', kind: 'orderCode', identity: '00000000000000000017' },
      ],
    });
    expect(details.actualClientOrderIds).not.toBe(error.actualClientOrderIds);
    expect(details.orderIdentities).not.toBe(error.orderIdentities);
    expect(details.orderIdentities?.[0]).not.toBe(error.orderIdentities[0]);
    expect(details.orderIdentities?.[0]).not.toHaveProperty('raw');
    error.orderIdentities[0].identity = 'mutated';
    expect(details.orderIdentities?.[0].identity).toBe('selected-parent');
  });

  it.each([
    { receipts: [{ symbol: 'BTCUSDT', kind: 'clientOrderIndex', identity: 9007199254740992 }] },
    { receipts: [{ symbol: 'BTCUSDT', kind: 'nonce', identity: 'secret-nonce' }] },
    { receipts: [{ symbol: 123, kind: 'clientOrderId', identity: 'actual' }] },
    { receipts: [null] },
  ])('drops malformed receipt arrays without fabricating identities', ({ receipts }) => {
    expect(
      serializeCommandError({
        message: 'Interrupted',
        orderIdentities: receipts,
        commandId: Number.MAX_SAFE_INTEGER + 1,
        actualClientOrderIds: ['valid', 123],
        receiptKey: 123,
      }),
    ).toEqual({ message: 'Interrupted' });
  });

  it('drops invalid metadata and does not coerce mixed identity arrays or unsafe counts', () => {
    expect(
      serializeCommandError({
        message: 'Rejected',
        name: 1,
        code: NaN,
        command: {},
        timeoutMs: Infinity,
        outcome: 'success',
        clientOrderId: 123,
        clientOrderIds: ['valid', 1],
        partialOrderIds: [null],
        partialOrderCount: -1,
        totalOrderCount: Number.MAX_SAFE_INTEGER + 1,
        failedOrderIndex: 0.5,
        arbitrary: () => {},
      }),
    ).toEqual({ message: 'Rejected' });
    expect(serializeCommandError({ message: 'Bad timeout', timeoutMs: 2147483648 })).toEqual({
      message: 'Bad timeout',
    });
  });

  it('preserves plain failures and read interruption without adding write uncertainty', () => {
    expect(serializeCommandError('plain failure')).toEqual({ message: 'plain failure' });
    expect(serializeCommandError(null)).toEqual({ message: 'null' });
    const error = Object.assign(new Error('Read interrupted'), { code: 'ENGINE_DISPOSED', command: 'fetchOHLCV' });
    expect(serializeCommandError(error)).not.toHaveProperty('outcome');
  });

  it('carries a classified venue failure across the worker boundary', () => {
    const error = Object.assign(new Error('Margin is insufficient.'), {
      kind: 'rejected',
      outcome: 'rejected',
      venueCode: '-2019',
      venueMessage: 'Margin is insufficient.',
      retryAfterMs: 1000,
    });
    const restored = deserializeCommandError(serializeCommandError(error));
    expect(restored).toMatchObject({
      message: 'Margin is insufficient.',
      kind: 'rejected',
      outcome: 'rejected',
      venueCode: '-2019',
      venueMessage: 'Margin is insufficient.',
      retryAfterMs: 1000,
    });
    expect(exchangeErrorCategory(restored)).toBe('order_rejected');
    expect(serializeCommandError({ message: 'x', kind: 'bogus', outcome: 'maybe', retryAfterMs: -1 })).toEqual({
      message: 'x',
    });
  });

  it('maps each classified kind onto the health categories and leaves unclassified errors to text', () => {
    expect(exchangeErrorCategory({ kind: 'authExpired' })).toBe('auth_expired');
    expect(exchangeErrorCategory({ kind: 'venueUnavailable' })).toBe('exchange_down');
    expect(exchangeErrorCategory(new Error('plain'))).toBeUndefined();
  });
});

describe('exchange error tags', () => {
  it('tags only the closed set of classified fields', () => {
    expect(
      exchangeErrorTags(
        Object.assign(new Error('rejected key'), { kind: 'authInvalid', outcome: 'rejected', venueCode: '10003' }),
      ),
    ).toEqual({ error_kind: 'authInvalid', error_category: 'auth_invalid', error_outcome: 'rejected', venue_code: '10003' });
    expect(exchangeErrorTags(Object.assign(new Error('x'), { kind: 'made-up', venueCode: 'has spaces and more' }))).toEqual({});
    expect(exchangeErrorTags('plain text')).toEqual({});
  });
});
