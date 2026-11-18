// Synthetic device for the opt-in production Session -> real WASM proof test.
import assert from 'node:assert/strict';
import { createPrivateKey, createPublicKey } from 'node:crypto';
import { createRequire } from 'node:module';
import { EngineHost } from '../../../wasm/js/host.mjs';

const seed = Number(process.argv[2]);
assert([7, 8].includes(seed));
const secret = Buffer.alloc(32, seed);
const privateKey = createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), secret]),
  format: 'der', type: 'pkcs8',
});
const key = createPublicKey(privateKey).export({ format: 'der', type: 'spki' }).subarray(-32);
const { WasmEngine } = createRequire(import.meta.url)(process.env.SAFE_CEX_BACKPACK_JOIN_WASM);
const engine = new WasmEngine('backpack', '{}', JSON.stringify({
  primary: { key: key.toString('base64'), secret: secret.toString('base64') },
}));
const host = new EngineHost(engine, {
  now: () => Date.now(), setTimeout, clearTimeout,
  fetch() { throw new Error('admission must not use venue HTTP'); },
  openSocket() { throw new Error('admission must not open a venue socket'); },
});
try {
  let raw = '';
  for await (const chunk of process.stdin) raw += chunk;
  const response = await host.hostedJoinProof(raw);
  assert.equal(host.started, false);
  assert.equal(host.hostedSignings.size, 0);
  process.stdout.write(JSON.stringify({ t: 'sign-res', d: response }));
} finally {
  host.dispose();
  engine.free();
}
