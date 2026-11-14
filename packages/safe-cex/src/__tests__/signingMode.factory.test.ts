import type { ExchangeOptions } from '../types';

import { describe, expect, it, vi } from 'vitest';

import { createExchange } from '../index';
import { virtualClock } from '../utils/virtualClock';

const state = vi.hoisted(() => ({ constructs: vi.fn() }));
vi.mock('../exchanges/rust/rust.exchange', () => ({
  RustEngineExchange: class {
    constructor(...args: unknown[]) {
      state.constructs(...args);
    }
  },
}));
vi.mock('../utils/virtualClock', () => ({ virtualClock: { start: vi.fn(), setServerTimeSource: vi.fn() } }));

describe('exchange factory signing mode boundary', () => {
  it.each(['', 'hosted', 'DELEGATED', null, false])(
    'refuses unknown direct option %s before construction',
    async (mode) => {
      await expect(
        createExchange('bybit', { key: '', secret: '', signingMode: mode as ExchangeOptions['signingMode'] }),
      ).rejects.toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
      expect(state.constructs).not.toHaveBeenCalled();
      expect(virtualClock.start).not.toHaveBeenCalled();
    },
  );
});
