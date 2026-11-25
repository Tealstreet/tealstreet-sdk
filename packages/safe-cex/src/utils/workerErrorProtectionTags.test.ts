import { describe, expect, it } from 'vitest';

import { workerErrorProtectionTags } from './workerErrorProtectionTags';

describe('workerErrorProtectionTags', () => {
  it('retains only a primitive protection fact across the native Error clone boundary', () => {
    const error = Object.freeze(Object.assign(new Error('replacement refused'), { protection: 'unprotected' }));
    const packet = structuredClone({ error, tags: workerErrorProtectionTags(error) });
    expect(packet.error).toBeInstanceOf(Error);
    expect(packet.error).not.toHaveProperty('protection');
    expect(packet.tags).toEqual({ worker_error_unprotected: true });
    expect(Object.isFrozen(error)).toBe(true);
  });

  it.each([null, undefined, 'UNPROTECTED: replacement refused', 1, true, {}])('does not infer protection from %j', (value) => {
    expect(workerErrorProtectionTags(value)).toEqual({});
  });

  it.each(['restored', 'UNPROTECTED', 'true', true, false, 1, null, undefined, { private: 'account' }])(
    'ignores non-loss protection values: %j', (protection) => {
      expect(workerErrorProtectionTags({ protection })).toEqual({});
    },
  );

  it('never emits arbitrary fields or throws on a foreign protection getter', () => {
    expect(workerErrorProtectionTags({ protection: 'unprotected', account: 'private-label', apiKey: 'synthetic-key' }))
      .toEqual({ worker_error_unprotected: true });
    const error = Object.defineProperty({}, 'protection', { get: () => { throw new Error('foreign getter'); } });
    expect(workerErrorProtectionTags(error)).toEqual({});
  });
});
