import { describe, expect, it } from 'vitest';

import floats from '../../../../sdk-rs/hosts/server/tests/data/delegated-order-fingerprint-floats.json';
import fixture from '../../../../sdk-rs/hosts/server/tests/data/delegated-orders-v1.json';
import {
  createDelegatedIntentId,
  delegatedClientOrderId,
  delegatedIntentFingerprint,
  delegatedIntentTime,
} from '../delegatedOrderFrames';

describe('frozen DELEGATED order contract', () => {
  it('matches Rust fingerprints for browser-serialized edge floats and large integers', () => {
    const inputs = [
      ['one-e21', 1e21],
      ['addition', 0.1 + 0.2],
      ['negative-zero', -0],
      ['smallest-subnormal', 5e-324],
      ['small-exponent', 1e-7],
      ['decimal-threshold', 1e-6],
      ['smallest-normal', 2.2250738585072014e-308],
      ['large-integer', 1e15],
      ['u32-plus-one', 2 ** 32 + 1],
      ['max-safe-integer', Number.MAX_SAFE_INTEGER],
      ['min-safe-integer', Number.MIN_SAFE_INTEGER],
      ['unsafe-integer', 2 ** 53],
    ] as const;
    expect(inputs.map(([name]) => name)).toEqual(floats.cases.map(({ name }) => name));
    for (const [index, [name, amount]] of inputs.entries()) {
      const vector = floats.cases[index];
      const bits = new DataView(new ArrayBuffer(8));
      bits.setFloat64(0, amount);
      expect(bits.getBigUint64(0).toString(16).padStart(16, '0'), name).toBe(vector.inputBits);
      // Exercise the browser's native serialization and the production helper,
      // which uses JSON.stringify for numbers, rather than a test formatter.
      const params = { amount };
      const wire = JSON.stringify(params);
      expect(wire, name).toBe(vector.paramsJson);
      const fingerprint = (value: unknown) =>
        delegatedIntentFingerprint(floats.accountName, floats.intentId, value, floats.seq);
      if (vector.fingerprint === null) {
        // The frozen contract refuses unsafe integers, including 1e21.
        expect(() => fingerprint(params), name).toThrow('Unsafe intent number');
        expect(() => fingerprint(JSON.parse(wire)), name).toThrow('Unsafe intent number');
      } else {
        expect(fingerprint(params), name).toBe(vector.fingerprint);
        expect(fingerprint(JSON.parse(wire)), name).toBe(vector.fingerprint);
      }
    }
  });
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
