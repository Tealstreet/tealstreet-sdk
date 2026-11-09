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
    testnet: true, remoteSigning: true,
    publicMode: { requested: false, remoteSigner: true },
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
