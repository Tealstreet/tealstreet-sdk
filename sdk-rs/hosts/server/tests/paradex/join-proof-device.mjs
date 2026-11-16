// Synthetic offline device for production Session -> real Paradex WASM proof.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { EngineHost } from '../../../wasm/js/host.mjs';

const scalar = Number(process.argv[2]);
assert([7, 8].includes(scalar));
const secret = Buffer.alloc(32);
secret[31] = scalar;
const { WasmEngine } = createRequire(import.meta.url)(process.env.SAFE_CEX_PARADEX_JOIN_WASM);
assert.equal(typeof WasmEngine.prototype.prepareJoinProof, 'function');
const engine = new WasmEngine('paradex', JSON.stringify({
  paradexAddress: '0x123', subkeyAddress: process.argv[3],
}), JSON.stringify({ primary: { secret: `0x${secret.toString('hex')}` } }));
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
  assert.equal(Buffer.from(response.joinProof.signature, 'base64').length, 64);
  process.stdout.write(JSON.stringify({ t: 'sign-res', d: response }));
} finally {
  host.dispose();
  engine.free();
}
