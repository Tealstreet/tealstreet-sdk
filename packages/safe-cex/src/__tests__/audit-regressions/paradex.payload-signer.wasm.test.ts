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

// Hosted Paradex is outside the Rust beta: the compiled registry refuses a private
// payloadSigner before any transport or signing work.
it('refuses hosted Paradex through the SDK and wasm host before I/O', async () => {
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
  const exchange = new RustEngineExchange(
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
  );
  try {
    await expect(exchange.start()).rejects.toMatchObject({
      kind: 'unsupported',
      outcome: 'notSent',
      message: expect.stringContaining('Rust paradex delegated signing is unsupported in this build'),
    });
    expect(io).not.toHaveBeenCalled();
    expect(payloadSigner).not.toHaveBeenCalled();
  } finally {
    exchange.dispose();
  }
}, 10000);
