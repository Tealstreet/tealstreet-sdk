// Real GRVT family WASM; the signing domain/hash and recovery oracle use Node/ethers.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import test from 'node:test';
import { TypedDataEncoder, recoverAddress } from 'ethers';
import { EngineHost } from '../hosts/wasm/js/host.mjs';
const { WasmEngine } = createRequire(import.meta.url)('../hosts/wasm/pkg/node/safe_cex_wasm.js');
const expected = '0x7e5f4552091a69125d5dfcb7b8c2659029395bdf';
function request() {
  return { id: 'grvt-attempt', type: 'join-proof', exchangeName: 'grvt', accountName: 'account',
    instanceId: 'registration', joinProof: { version: 1, statement: {
      kind: 'tealstreet-admission-v1', attempt: 'grvt-attempt', identityDigest: Array(32).fill(17),
      nonce: Array(32).fill(34), issuedAtMs: 1000,
    } } };
}
function digest(statement) {
  const attempt = Buffer.from(statement.attempt);
  const len = Buffer.alloc(4); len.writeUInt32BE(attempt.length);
  const time = Buffer.alloc(8); time.writeBigUInt64BE(BigInt(statement.issuedAtMs));
  const challenge = createHash('sha256').update(Buffer.concat([
    Buffer.from('tealstreet/admission/challenge/v1\0'), len, attempt,
    Buffer.from(statement.identityDigest), Buffer.from(statement.nonce), time,
  ])).digest('hex');
  return TypedDataEncoder.hash({ name: 'Tealstreet Admission', version: '1' },
    { Admission: [{ name: 'challenge', type: 'bytes32' }] }, { challenge: `0x${challenge}` });
}
function rig() {
  let now = 1000;
  const engine = new WasmEngine('grvt', JSON.stringify({ grvtTradingAccountId: '42' }),
    JSON.stringify({ primary: { key: 'synthetic-grvt-api-key', secret: `${'00'.repeat(31)}01` } }));
  const host = new EngineHost(engine, { now: () => now, setTimeout, clearTimeout,
    fetch() { throw new Error('GRVT possession proof cannot login or send HTTP'); },
    openSocket() { throw new Error('GRVT possession proof cannot open sockets'); },
  });
  return { engine, host, now(value) { now = value; }, dispose() { host.dispose(); engine.free(); } };
}
test('GRVT standalone WASM proves the scalar under the admission-only domain without startup I/O', async () => {
  const r = rig();
  try {
    const input = request();
    const response = await r.host.hostedJoinProof(JSON.stringify(input));
    assert.deepEqual(Object.keys(response).sort(), ['id', 'joinProof']);
    assert.equal(response.id, input.id);
    assert.equal(response.joinProof.version, 1);
    const signature = `0x${Buffer.from(response.joinProof.signature, 'base64').toString('hex')}`;
    assert.equal(Buffer.from(response.joinProof.signature, 'base64').length, 65);
    assert.equal(recoverAddress(digest(input.joinProof.statement), signature).toLowerCase(), expected);
    for (const field of ['attempt', 'identityDigest', 'nonce', 'issuedAtMs']) {
      const changed = structuredClone(input.joinProof.statement);
      if (field === 'attempt') changed.attempt = 'next-attempt';
      else if (field === 'issuedAtMs') changed.issuedAtMs++;
      else changed[field][0]++;
      assert.notEqual(recoverAddress(digest(changed), signature).toLowerCase(), expected, field);
    }
    assert.equal(r.host.started, false);
    assert.equal(r.host.hostedSignings.size, 0);
  } finally { r.dispose(); }
});
test('GRVT standalone rejects malformed, foreign and expired proof requests and consumes captures', async () => {
  const r = rig();
  try {
    const raw = JSON.stringify(request());
    for (const invalid of [
      raw.replace('"version":1', '"version":1,"version":1'),
      raw.replace('"grvt"', '"backpack"'),
      raw.replace('"join-proof"', '{"join-proof":null}'),
      raw.replace('"joinProof":', '"keyRef":"foreign-key","joinProof":'),
      '{"kind":"http","payload":"create_order"}',
    ]) await assert.rejects(r.host.hostedJoinProof(invalid));
    for (const now of [999, 16001]) { r.now(now); await assert.rejects(r.host.hostedJoinProof(raw)); }
    const capture = JSON.parse(r.engine.prepareJoinProof(raw, 1000));
    assert.equal(JSON.parse(r.engine.localHostedSigning(capture.id, 1000)).joinProof.version, 1);
    assert.throws(() => r.engine.localHostedSigning(capture.id, 1000));
    assert.equal(r.host.hostedSignings.size, 0);
  } finally { r.dispose(); }
});
