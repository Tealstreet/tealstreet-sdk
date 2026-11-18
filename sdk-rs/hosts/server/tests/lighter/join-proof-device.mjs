// Synthetic offline device for the production Session -> real WASM proof test.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { EngineHost } from '../../../wasm/js/host.mjs';

const seed = Number(process.argv[2]);
const venue = process.argv[3];
assert([7, 8].includes(seed));
assert(['lighter', 'lighter-robinhood'].includes(venue));
const secret = Buffer.alloc(40);
secret[0] = seed;
const { WasmEngine } = createRequire(import.meta.url)(process.env.SAFE_CEX_LIGHTER_JOIN_WASM);
assert.equal(typeof WasmEngine.prototype.prepareJoinProof, 'function');
const engine = new WasmEngine(venue, JSON.stringify({ applicationId: '42', apiKeyIndex: '1' }), JSON.stringify({
  primary: { key: 'synthetic-public-account', secret: secret.toString('hex') },
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
  assert.equal(Buffer.from(response.joinProof.signature, 'base64').length, 80);
  process.stdout.write(JSON.stringify({ t: 'sign-res', d: response }));
} finally {
  host.dispose();
  engine.free();
}
