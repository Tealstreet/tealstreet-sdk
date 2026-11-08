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

it.each(modes)('native public construction ignores unused signing intent: %j', (mode) => {
  for (const publicOptions of [
    { key: '', accountStartup: true, ...mode },
    { key: 'unused-key', ...mode, publicMode: undefined, publicDataOnly: true },
    { key: 'unused-key', accountStartup: true, ...mode, publicMode: { ...mode.publicMode, requested: true } },
  ]) {
    const engine = new wasm.WasmEngine('polymarket', JSON.stringify(publicOptions));
    try {
      if (typeof engine.construction !== 'function') throw new Error('Missing native construction record');
      const construction: unknown = JSON.parse(engine.construction());
      if (publicOptions.publicMode) {
        expect(construction).toMatchObject({ publicDataOnly: true, canSign: publicOptions.key.length > 0 });
      } else {
        // Legacy options have no resolved-mode record; successful construction covers the flag.
        expect(construction).toHaveProperty('bridge');
      }
    } finally {
      engine.free?.();
    }
  }
});

it.each(modes)('native private construction retains the typed custody refusal: %j', (mode) => {
  let failure: unknown;
  try {
    const engine = new wasm.WasmEngine('polymarket', JSON.stringify({ key: '12'.repeat(32), ...mode }));
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
    throw new Error('Expected the native typed custody refusal');
  }
  const refusal: unknown = JSON.parse(failure.venueError);
  expect(refusal).toMatchObject({
    kind: 'unsupported',
    outcome: 'notSent',
    message: expect.stringContaining('user WebSocket requires the API secret'),
  });
});

// TxFlow joins #3317's public-before-custody matrix; no venue-specific setup bypass.
it.each(modes)('TxFlow public construction ignores unused private custody: %j', (mode) => {
  for (const publicOptions of [
    { ...mode },
    { ...mode, applicationId: '0x1111111111111111111111111111111111111111', publicMode: { ...mode.publicMode, requested: true } },
    { ...mode, applicationId: '0x1111111111111111111111111111111111111111', publicMode: undefined, publicDataOnly: true },
  ]) {
    for (const unused of [
      { secret: 'stale-invalid-scalar' },
      { readCredentials: { key: 'unused', secret: 'unused' } },
      { signing: { primary: { credential: 'unused', signing: 'unused' } } },
    ]) {
      const engine = new wasm.WasmEngine('txflow', JSON.stringify({ testnet: true, accountStartup: true, ...publicOptions, ...unused }));
      try {
        if (typeof engine.construction !== 'function') throw new Error('Missing construction record');
        const construction: unknown = JSON.parse(engine.construction());
        if (publicOptions.publicMode) expect(construction).toMatchObject({ publicDataOnly: true });
      } finally { engine.free?.(); }
    }
  }
});
