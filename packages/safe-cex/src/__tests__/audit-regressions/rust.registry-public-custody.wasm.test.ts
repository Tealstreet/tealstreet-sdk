import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { EngineHost } from '@tealstreet/safe-cex-wasm/host';
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

const unexpectedTransport = (): never => { throw new Error('Admission must not perform I/O'); };
const admissionTransport = {
  now: () => 0,
  connect: unexpectedTransport,
  request: unexpectedTransport,
  setTimeout: unexpectedTransport,
  clearTimeout: () => {},
};

it.each([null, true])('EngineHost accepts public TxFlow refs without a provider (requested=%s)', (requested) => {
  const engine = new wasm.WasmEngine('txflow', JSON.stringify({
    testnet: true, payloadSigner: true, publicMode: { requested, payloadSigner: true },
    ...(requested === true ? { applicationId: '0x1111111111111111111111111111111111111111' } : {}),
    signing: { primary: { credential: 'unused', signing: 'unused' } },
  }));
  // Construct the host first: checking the native constructor alone missed this regression.
  const host = new EngineHost(engine, admissionTransport, {}, { ownsEngine: true });
  try {
    expect(engine.requiresExternalSigner?.()).toBe(false);
  } finally { host.dispose(); }
});

it.each(['bingx', 'weex', 'bitget', 'okx'])('%s private references require an EngineHost provider', (venue) => {
  const engine = new wasm.WasmEngine(venue, JSON.stringify({
    // WEEX's callback policy is closed; retained trusted-client refs still need a provider.
    testnet: true, remoteSigning: venue !== 'weex',
    publicMode: { requested: false, remoteSigner: venue !== 'weex' },
    signing: { primary: { credential: 'private-account', signing: 'private-signing',
      ...(venue === 'bingx' ? {} : { passphrase: 'private-passphrase-ref' }) } },
  }));
  try {
    if (typeof engine.construction !== 'function') throw new Error('Missing construction record');
    const construction: unknown = JSON.parse(engine.construction());
    expect(construction).toMatchObject({ canSign: true, publicDataOnly: false });
    expect(() => new EngineHost(engine, admissionTransport)).toThrow('Engine signing provider is required');
    expect(engine.requiresExternalSigner?.()).toBe(true);
  } finally { engine.free?.(); }
});

const expectRefused = (
  options: unknown,
  venue = 'polymarket',
  message = 'user WebSocket requires the API secret',
) => {
  let failure: unknown;
  try {
    const engine = new wasm.WasmEngine(venue, JSON.stringify(options));
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
    throw new Error('Expected the typed private custody refusal');
  }
  const refusal: unknown = JSON.parse(failure.venueError);
  expect(refusal).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
  expect(refusal).toMatchObject({ message: expect.stringContaining(message) });
};

it('WEEX callback intent still refuses before an EngineHost can use it', () => {
  expectRefused({
    testnet: true, remoteSigning: true,
    publicMode: { requested: false, remoteSigner: true },
    signing: { primary: { credential: 'private-account', signing: 'private-signing', passphrase: 'private-passphrase-ref' } },
  }, 'weex', 'Rust weex remote signing is unsupported in this build');
});

it.each(modes)('client public construction delegates beta admission to the SDK: %j', (mode) => {
  for (const publicOptions of [
    { key: '', accountStartup: true, ...mode },
    { key: 'unused-key', ...mode, publicMode: undefined, publicDataOnly: true },
    { key: 'unused-key', accountStartup: true, ...mode, publicMode: { ...mode.publicMode, requested: true } },
  ]) {
    const engine = new wasm.WasmEngine('polymarket', JSON.stringify(publicOptions));
    try {
      const actions: unknown = JSON.parse(engine.start(1000));
      expect(actions).toContainEqual(expect.objectContaining({ op: 'managedHttp', intent: 'public' }));
      expect(actions).not.toContainEqual(expect.objectContaining({ intent: 'primary' }));
    } finally {
      engine.free?.();
    }
  }
});

it.each(modes)('native private construction retains the typed custody refusal: %j', (mode) => {
  expectRefused({ key: '12'.repeat(32), ...mode });
});
