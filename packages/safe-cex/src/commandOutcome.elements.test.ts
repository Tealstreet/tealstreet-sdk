import type { VenueErrorDetail } from '@tealstreet/safe-cex-wasm/host';

import { describe, expect, it } from 'vitest';

import fixture from '../../../sdk-rs/engine/tests/fixtures/write-plan-error.json';
import { deserializeCommandError, serializeCommandError } from './commandOutcome';

const expected: NonNullable<VenueErrorDetail['elements']> = [
  { index: 0, clientOrderId: 'first', status: 'placed', orderIds: ['native-0'] },
  {
    index: 1,
    clientOrderId: 'second',
    status: 'notSent',
    error: { message: 'rejected', outcome: 'rejected', venueCode: '-1013' },
  },
  {
    index: 2,
    clientOrderId: 'third',
    status: 'unknown',
    error: { message: 'lost acknowledgement', outcome: 'unknown' },
  },
  { index: 3, status: 'notSent' },
];

describe('write-plan command outcomes', () => {
  it('round trips the Rust-serialized fixture through the worker allowlist', () => {
    expect(fixture.elements).toEqual(expected);
    const restored = deserializeCommandError(structuredClone(serializeCommandError(fixture)));
    expect(restored.elements).toEqual(expected);
    expect(restored.outcome).toBe('unknown');
    expect(restored.elements).not.toBe(fixture.elements);
  });
});

describe('write-plan validation at worker boundaries', () => {
  it.each([
    null,
    [],
    { index: -1, status: 'notSent' },
    { index: 1.5, status: 'notSent' },
    { index: 0, clientOrderId: 7, status: 'notSent' },
    { index: 0, status: 'placed', orderIds: [7] },
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
