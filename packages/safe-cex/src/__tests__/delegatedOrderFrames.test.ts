import { describe, expect, it } from 'vitest';

import fixture from '../../../../sdk-rs/hosts/server/tests/data/delegated-orders-v1.json';
import {
  createDelegatedIntentId,
  delegatedClientOrderId,
  delegatedIntentFingerprint,
  delegatedIntentTime,
} from '../delegatedOrderFrames';

describe('frozen DELEGATED order contract', () => {
  it('pins UUIDv7 epoch, Bybit client id, and canonical payload fingerprint', () => {
    expect(createDelegatedIntentId(fixture.createdMs, (bytes) => bytes.fill(1))).toBe(fixture.key);
    expect(delegatedIntentTime(fixture.key)).toBe(fixture.createdMs);
    expect(delegatedClientOrderId(fixture.key)).toBe(fixture.key);
    expect(fixture.key.length).toBe(36);
    expect(fixture.key.length).toBeLessThanOrEqual(128);
    expect(fixture.key).toMatch(/^[0-9a-f-]{36}$/);
    expect(delegatedIntentFingerprint('main', fixture.key, fixture.submit.d.params, 1)).toBe(fixture.fingerprint);
    expect(delegatedIntentFingerprint('main', fixture.key, { ...fixture.submit.d.params, amount: 0.002 }, 1)).not.toBe(
      fixture.fingerprint,
    );
    const reordered = Object.fromEntries(Object.entries(fixture.submit.d.params).reverse());
    expect(delegatedIntentFingerprint('main', fixture.key, reordered, 1)).toBe(fixture.fingerprint);
  });
  it('rejects noncanonical keys, unsafe numeric payloads and ambiguous strings', () => {
    for (const key of [fixture.key.toUpperCase(), fixture.key.replace('-7', '-4'), 'x'.repeat(129)]) {
      expect(() => delegatedIntentTime(key)).toThrow();
    }
    for (const params of [{ amount: Infinity }, { amount: 2 ** 54 }, { key: '\ud800' }]) {
      expect(() => delegatedIntentFingerprint('main', fixture.key, params, 1)).toThrow();
    }
    expect(() => delegatedIntentFingerprint('main', fixture.key, { memo: 'x'.repeat(65536) }, 1)).toThrow();
    expect([fixture.submit.t, fixture.ack.t, fixture.outcome.t, fixture.cancelQueued.t]).toEqual([
      'order.submit',
      'order.ack',
      'order.outcome',
      'order.cancelQueued',
    ]);
  });
});
