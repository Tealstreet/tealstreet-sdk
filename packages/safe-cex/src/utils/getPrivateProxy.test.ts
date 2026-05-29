import { afterEach, describe, expect, it } from 'vitest';

import { getPrivateProxy } from './getPrivateProxy';

const originalWindow = (globalThis as any).window;
const originalSelf = (globalThis as any).self;

const restoreGlobal = (key: 'window' | 'self', value: unknown) => {
  if (value === undefined) {
    delete (globalThis as any)[key];
    return;
  }

  (globalThis as any)[key] = value;
};

describe('getPrivateProxy', () => {
  afterEach(() => {
    restoreGlobal('window', originalWindow);
    restoreGlobal('self', originalSelf);
  });

  it('returns proxy url for server-side runtime when requested', () => {
    delete (globalThis as any).window;
    delete (globalThis as any).self;

    expect(getPrivateProxy('https://openapi.blofin.com', true)).toBe(
      'https://fsjnpy143pbz.asia.tealstreet.io/api/exchange_proxy',
    );
  });

  it('still respects requiresCors=false in server-side runtime', () => {
    delete (globalThis as any).window;
    delete (globalThis as any).self;

    expect(getPrivateProxy('https://openapi.blofin.com', false)).toBe('');
  });

  it('returns worker proxy url when running in webworker runtime', () => {
    delete (globalThis as any).window;
    (globalThis as any).self = {};

    expect(getPrivateProxy('https://openapi.blofin.com', true)).toBe(
      'https://fsjnpy143pbz.asia.tealstreet.io/api/exchange_proxy',
    );
  });

  it('still respects requiresCors=false in webworker runtime', () => {
    delete (globalThis as any).window;
    (globalThis as any).self = {};

    expect(getPrivateProxy('https://openapi.blofin.com', false)).toBe('');
  });
});
