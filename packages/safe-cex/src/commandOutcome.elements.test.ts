import type { VenueErrorDetail } from '@tealstreet/safe-cex-wasm/host';

import { describe, expect, it } from 'vitest';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import fixture from '../../../sdk-rs/engine/tests/fixtures/write-plan-error.json';
import { deserializeCommandError, serializeCommandError } from './commandOutcome';

const expected: NonNullable<VenueErrorDetail['elements']> = [
  { index: 0, clientOrderId: 'first', status: 'placed', orderIds: ['native-0'] },
  { index: 1, clientOrderId: 'cancel', status: 'applied', effect: 'cancel', targetOrderId: 'old-order' },
  { index: 2, status: 'applied', effect: 'settings', positionKey: 'BTCUSD:long' },
  { index: 3, clientOrderId: 'deferred', status: 'applied', effect: 'deferredChild' },
  {
    index: 4,
    clientOrderId: 'second',
    status: 'notSent',
    error: { message: 'rejected', outcome: 'rejected', venueCode: '-1013' },
  },
  {
    index: 5,
    clientOrderId: 'third',
    status: 'unknown',
    error: { message: 'lost acknowledgement', outcome: 'unknown' },
  },
  { index: 6, status: 'notSent' },
];

describe('write-plan command outcomes', () => {
  it('round trips the Rust-serialized fixture through the worker allowlist', () => {
    expect(fixture.elements).toEqual(expected);
    const restored = deserializeCommandError(structuredClone(serializeCommandError(fixture)));
    expect(restored.elements).toEqual(expected);
    expect(restored.outcome).toBe('unknown');
    expect(restored.elements).not.toBe(fixture.elements);
    const hostError = new ExchangeError(fixture.message, { ...fixture, elements: expected, outcome: 'unknown' });
    const legacy = deserializeCommandError(structuredClone(serializeCommandError(hostError)));
    expect(legacy.elements).toEqual(expected);
    expect(legacy.partialOrderIds).toEqual(['native-0']);
    expect(legacy.totalOrderCount).toBe(3);
    expect(legacy.failedOrderIndex).toBe(1);
  });
});

describe('write-plan validation at worker boundaries', () => {
  it('keeps unattributed accepted evidence unknown through the worker boundary', () => {
    const error = deserializeCommandError(
      structuredClone(
        serializeCommandError({
          message: 'partial refusal',
          elements: [
            {
              index: 0,
              status: 'notSent',
              error: {
                message: 'refused',
                outcome: 'rejected',
                partial: { orderIds: ['accepted-native'], total: 3, failedIndex: 2 },
              },
            },
          ],
        }),
      ),
    );
    expect(error.elements).toEqual([
      {
        index: 0,
        status: 'unknown',
        error: {
          message: 'refused',
          outcome: 'unknown',
          partial: { orderIds: ['accepted-native'], total: 3, failedIndex: 2 },
        },
      },
    ]);
  });

  it('never restores a notSent tag around an unknown nested outcome', () => {
    const error = deserializeCommandError(
      structuredClone(
        serializeCommandError({
          message: 'lost',
          elements: [{ index: 0, status: 'notSent', error: { message: 'lost', outcome: 'unknown' } }],
        }),
      ),
    );
    expect(error.elements).toEqual([{ index: 0, status: 'unknown', error: { message: 'lost', outcome: 'unknown' } }]);
  });
  it.each([
    null,
    [],
    { index: -1, status: 'notSent' },
    { index: 1.5, status: 'notSent' },
    { index: 0, clientOrderId: 7, status: 'notSent' },
    { index: 0, status: 'placed', orderIds: [7] },
    { index: 0, status: 'placed', orderIds: [] },
    { index: 0, status: 'placed', orderIds: [''] },
    { index: 0, status: 'placed', orderIds: Array(1) },
    { index: 0, status: 'applied', effect: 'cancel', orderIds: ['invented'] },
    { index: 0, status: 'applied', effect: 'cancel', orderIds: [] },
    { index: 0, status: 'applied', effect: 'cancel', targetOrderId: '' },
    { index: 0, status: 'applied', effect: 'settings', positionKey: '' },
    { index: 0, status: 'applied', effect: 'bogus' },
    { index: 0, status: 'unknown' },
    { index: 0, status: 'notSent', error: { message: null } },
    { index: 0, status: 'rejected' },
  ])('drops a malformed array completely: %j', (row) => {
    expect(serializeCommandError({ message: 'stopped', elements: [expected[0], row] })).not.toHaveProperty('elements');
  });

  it('clones nested identities and strips unallowlisted error material', () => {
    const input = {
      message: 'stopped',
      elements: [
        {
          index: 0,
          status: 'unknown',
          clientOrderId: 'first',
          request: 'excluded',
          error: { message: 'network', kind: 'network', outcome: 'unknown', venueCode: '-1', request: 'excluded' },
        },
      ],
    };
    const restored = deserializeCommandError(structuredClone(serializeCommandError(input)));
    expect(restored.elements).toEqual([
      {
        index: 0,
        status: 'unknown',
        clientOrderId: 'first',
        error: { message: 'network', kind: 'network', outcome: 'unknown', venueCode: '-1' },
      },
    ]);
    expect(restored.elements?.[0]).not.toBe(input.elements[0]);
  });
});
