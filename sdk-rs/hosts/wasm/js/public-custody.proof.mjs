// Called by native Rust tests with flags from the actual WasmEngine constructor.
// Only the host admission boundary is bridged; no WASM artifact or transport I/O.
import assert from 'node:assert/strict';
import { EngineHost } from './host.mjs';

const row = JSON.parse(process.argv[2]);
assert.equal(row.construction.publicDataOnly, row.expectedPublic);
if (!row.expectedPublic) assert.equal(row.construction.canSign, true);
const engine = { requiresExternalSigner: () => row.requiresExternalSigner, command: () => '[]' };
if (row.expectedRequired) {
  assert.throws(() => new EngineHost(engine, {}), (error) =>
    error.kind === 'invalidInput' && error.outcome === 'notSent'
      && error.message === 'Engine signing provider is required');
} else {
  assert.equal(row.construction.publicDataOnly, true);
  const host = new EngineHost(engine, {});
  assert.equal(row.requiresExternalSigner, false);
  assert.equal(host.signer, undefined);
  host.dispose();
}
