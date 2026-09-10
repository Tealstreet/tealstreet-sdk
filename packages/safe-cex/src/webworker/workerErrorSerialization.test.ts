import { describe, expect, it } from 'vitest';

import { serializeWorkerErrorForSentry } from './workerErrorSerialization';

describe('serializeWorkerErrorForSentry', () => {
  it('materializes non-enumerable Error fields into an enumerable payload', () => {
    const error = new Error('worker failed');
    error.stack = 'Error: worker failed\n    at worker.ts:1:1';

    const serialized = serializeWorkerErrorForSentry(error, 'fallback');

    expect(serialized).toEqual({
      message: 'worker failed',
      name: 'Error',
      stack: 'Error: worker failed\n    at worker.ts:1:1',
    });
    expect(Object.keys(serialized).sort()).toEqual(['message', 'name', 'stack']);
  });

  it('materializes DOMException fields that otherwise clone as an empty object', () => {
    const error = new DOMException('The requested file could not be read', 'NotReadableError');

    const serialized = serializeWorkerErrorForSentry(error, 'fallback');

    expect(serialized.message).toBe('The requested file could not be read');
    expect(serialized.name).toBe('NotReadableError');
    expect(Object.keys(serialized)).toEqual(expect.arrayContaining(['message', 'name']));
  });

  it('uses fallback values for non-error rejection reasons', () => {
    const serialized = serializeWorkerErrorForSentry('plain rejection', 'plain rejection');

    expect(serialized).toEqual({
      message: 'plain rejection',
      name: 'Error',
    });
  });
});
