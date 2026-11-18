vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { afterEach, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import vectors from '../../../../../sdk-rs/venues/paradex/tests/sign-vectors.json';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

afterEach(() => {
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
});

// Paradex declares remote signing with no PayloadSigner lifecycle: a private
// payloadSigner refuses at construction, before any engine, transport or signing.
it('refuses a Paradex payloadSigner through the SDK before I/O', () => {
  const vector = vectors.cases[0];
  const io = vi.fn(() => {
    throw new Error('I/O must not start');
  });
  const transport: Transport = {
    now: () => vector.now,
    setTimeout,
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
    connect: io,
    request: io,
  };
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => transport);
  const payloadSigner = vi.fn(() => ({ signature: 'never-requested' }));
  expect(() => new RustEngineExchange(
    'paradex',
    {
      key: 'public-subkey-only',
      secret: '',
      paradexAddress: vector.address,
      subkeyAddress: vector.subkey,
      tealstreetProxy: false,
      tealstreetTunnel: false,
      payloadSigner,
    },
    new DefaultStore(),
  )).toThrow(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent', message: expect.stringContaining('PayloadSigner') }));
  expect(io).not.toHaveBeenCalled();
  expect(payloadSigner).not.toHaveBeenCalled();
});
