import type { ExchangeOptions } from '../../types';

import { expect, it, vi } from 'vitest';

import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { createExchange } from '../../index';
import { DefaultStore } from '../../store/store.base';

// restored by #3502; #3317 dropped a live venue without a product decision.
it.each(['polymarket'] as const)(
  '%s refuses account admission before inspecting public-mode or signer options',
  (name) => {
    const inspected = vi.fn(() => {
      throw new Error('OUT options must remain uninspected');
    });
    const options = Object.defineProperties(
      {},
      {
        publicDataOnly: { enumerable: true, get: inspected },
        remoteSigner: { enumerable: true, get: inspected },
        payloadSigner: { enumerable: true, get: inspected },
      },
    ) as ExchangeOptions;
    const unsupported = expect.objectContaining({
      message: `${name} is unsupported in this build`,
      kind: 'unsupported',
      outcome: 'notSent',
    });
    expect(() => assertBetaAccount(name, options)).toThrowError(unsupported);
    expect(inspected).not.toHaveBeenCalled();
  },
);

// restored by #3502; #3317 dropped a live venue without a product decision.
it.each(['polymarket'] as const)(
  '%s public SDK construction refuses before loading WASM or calling a signer',
  async (name) => {
    const load = vi.fn();
    const sign = vi.fn();
    const options = {
      key: '',
      secret: '',
      publicDataOnly: true,
      remoteSigner: { sign, isAvailable: vi.fn(), onAvailabilityChange: vi.fn() },
    };
    const unsupported = expect.objectContaining({
      message: `${name} is unsupported in this build`,
      kind: 'unsupported',
      outcome: 'notSent',
    });
    setWasmEngineLoader(load);
    try {
      expect(() => new RustEngineExchange(name, options, new DefaultStore())).toThrowError(unsupported);
      await expect(createExchange(name, options)).rejects.toEqual(unsupported);
      expect(sign).not.toHaveBeenCalled();
      expect(options.remoteSigner.isAvailable).not.toHaveBeenCalled();
      expect(options.remoteSigner.onAvailabilityChange).not.toHaveBeenCalled();
      expect(load).not.toHaveBeenCalled();
    } finally {
      setWasmEngineLoader(loadNodeEngine);
    }
  },
);

for (const entry of ['constructor', 'factory']) {
  // restored by #3502; #3317 dropped a live venue without a product decision.
  it.each(['polymarket'] as const)(
    `%s ${entry} refuses before reading accessor-backed options`,
    async (name) => {
      const inspected = vi.fn(() => {
        throw new Error('OUT constructor options must remain uninspected');
      });
      const options: ExchangeOptions = { key: '', secret: '' };
      Object.defineProperties(options, {
        publicDataOnly: { enumerable: true, get: inspected },
        remoteSigner: { enumerable: true, get: inspected },
        payloadSigner: { enumerable: true, get: inspected },
      });
      const unsupported = expect.objectContaining({
        message: `${name} is unsupported in this build`,
        kind: 'unsupported',
        outcome: 'notSent',
      });
      const construct = async () => entry === 'constructor'
        ? new RustEngineExchange(name, options, new DefaultStore())
        : createExchange(name, options);
      await expect(construct()).rejects.toEqual(unsupported);
      expect(inspected).not.toHaveBeenCalled();
    },
  );
}

it('admitted public accounts still ignore unused signer contents', () => {
  const sign = vi.fn();
  expect(() =>
    assertBetaAccount('bybit', {
      key: '',
      publicDataOnly: true,
      remoteSigner: { sign, isAvailable: vi.fn(), onAvailabilityChange: vi.fn() },
    }),
  ).not.toThrow();
  expect(sign).not.toHaveBeenCalled();
});
