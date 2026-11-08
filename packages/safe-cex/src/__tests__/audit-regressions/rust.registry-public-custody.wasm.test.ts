import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { beforeAll, expect, it } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';

let wasm: WasmEngineModule;
beforeAll(async () => {
  wasm = await loadNodeEngine();
});

const modes = [
  { remoteSigning: true, publicMode: { requested: null, remoteSigner: true } },
  { payloadSigner: true, publicMode: { requested: null, payloadSigner: true } },
];

const expectRefused = (options: unknown, protocol = false) => {
  let failure: unknown;
  try {
    const engine = new wasm.WasmEngine('polymarket', JSON.stringify(options));
    engine.free?.();
  } catch (error) {
    failure = error;
  }
  if (
    typeof failure !== 'object' ||
    failure === null ||
    'venueError' in failure === false ||
    typeof failure.venueError !== 'string'
  ) {
    throw new Error('Expected the native typed beta refusal');
  }
  const refusal: unknown = JSON.parse(failure.venueError);
  expect(refusal).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
  if (protocol)
    expect(refusal).toMatchObject({ message: expect.stringContaining('user WebSocket requires the API secret') });
};

it.each(modes)('native public construction keeps SDK OUT admission: %j', (mode) => {
  for (const publicOptions of [
    { key: '', accountStartup: true, ...mode },
    { key: 'unused-key', ...mode, publicMode: undefined, publicDataOnly: true },
    { key: 'unused-key', accountStartup: true, ...mode, publicMode: { ...mode.publicMode, requested: true } },
  ]) {
    expectRefused(publicOptions);
  }
});

it.each(modes)('native private construction retains the typed custody refusal: %j', (mode) => {
  expectRefused({ key: '12'.repeat(32), ...mode }, true);
});
