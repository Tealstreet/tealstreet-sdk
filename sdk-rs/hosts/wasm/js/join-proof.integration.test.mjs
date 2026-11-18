// Actual WASM/local-provider boundary; fixed public oracles predate this handler.
import assert from 'node:assert/strict';
import { createPublicKey, verify } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';
import { EngineHost } from './host.mjs';
const { WasmEngine } = createRequire(import.meta.url)('../pkg/node/safe_cex_wasm.js');
const vectors = JSON.parse(readFileSync(new URL('../../../crypto/tests/fixtures/join-proof.json', import.meta.url)));
const vector = vectors.cases.find(item => item.facts.venue === 'backpack');
const publicBytes = Buffer.from(vector.facts.credential.publicKey.slice(2), 'hex');
const publicKey = createPublicKey({ key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), publicBytes]), format: 'der', type: 'spki' });
const message = Buffer.concat([Buffer.from('tealstreet/admission/ed25519/v1\0'), Buffer.from(vector.challengeDigest, 'hex')]);
const request = () => ({ id: 'captured-attempt', type: 'join-proof', exchangeName: 'backpack', accountName: 'account',
  instanceId: 'registration', joinProof: { version: 1, statement: structuredClone(vector.statement) } });

function rig() {
  let now = 1000;
  const key = publicBytes.toString('base64');
  const engine = new WasmEngine('backpack', '{}', JSON.stringify({ primary: { key, secret: Buffer.alloc(32, 1).toString('base64') } }));
  const host = new EngineHost(engine, { now: () => now,
    setTimeout, clearTimeout,
    fetch() { throw new Error('proof cannot perform HTTP'); },
    openSocket() { throw new Error('proof cannot open sockets'); },
  });
  return { engine, host, now(value) { now = value; }, dispose() { host.dispose(); engine.free(); } };
}

test('actual WASM returns the frozen proof envelope signed by its local provider, verified independently by Node/OpenSSL', async () => {
  const r = rig();
  try {
    const response = await r.host.hostedJoinProof(JSON.stringify(request()));
    assert.deepEqual(Object.keys(response).sort(), ['id', 'joinProof']);
    assert.equal(response.id, 'captured-attempt');
    assert.equal(response.joinProof.version, 1);
    const signature = Buffer.from(response.joinProof.signature, 'base64');
    assert.equal(signature.length, 64);
    assert(verify(null, message, publicKey, signature));
    assert(!verify(null, Buffer.from('instruction=subscribe&timestamp=1000&window=5000'), publicKey, signature));
    assert.equal(r.host.started, false);
    assert.equal(r.host.hostedSignings.size, 0);
  } finally { r.dispose(); }
});

test('actual WASM rejects duplicate/object discriminants, venue changes and transaction-shaped requests before signing', async () => {
  const r = rig();
  try {
    const valid = JSON.stringify(request());
    for (const raw of [
      valid.replace('"version":1', '"version":1,"version":1'),
      valid.replace('"join-proof"', '{"join-proof":null}'),
      valid.replace('"tealstreet-admission-v1"', '{"tealstreet-admission-v1":null}'),
      valid.replace('"backpack"', '"hyperliquid"'),
      valid.replace('"joinProof":', '"keyRef":"caller-key","joinProof":'),
      '{"kind":"http","payload":"withdraw"}',
      '["join-proof","caller-digest"]',
    ]) {
      await assert.rejects(r.host.hostedJoinProof(raw));
      assert.equal(r.host.hostedSignings.size, 0);
    }
    for (const now of [999, 16001]) {
      r.now(now);
      await assert.rejects(r.host.hostedJoinProof(valid));
    }
  } finally { r.dispose(); }
});

test('proof and ordinary signing use one capture identity pool, retain their formats, and retire on use/cancel/timeout', () => {
  const r = rig();
  try {
    const raw = JSON.stringify(request());
    const proof = JSON.parse(r.engine.prepareJoinProof(raw, 1000));
    const ordinary = JSON.parse(r.engine.prepareHostedSigning(JSON.stringify({ kind: 'wsAuth',
      toSign: JSON.stringify({ slot: 'primary', payload: 'instruction=subscribe&timestamp=1000&window=5000' }), outputFormat: 'backpack-ed25519' }), 1000));
    assert.notEqual(proof.id, ordinary.id);
    assert.equal(JSON.parse(r.engine.localHostedSigning(proof.id, 1000)).joinProof.version, 1);
    assert.equal(typeof JSON.parse(r.engine.localHostedSigning(ordinary.id, 1000)).signature, 'string');
    assert.throws(() => r.engine.localHostedSigning(proof.id, 1000));
    const canceled = JSON.parse(r.engine.prepareJoinProof(raw, 1000));
    r.engine.cancelHostedSigning(canceled.id);
    assert.throws(() => r.engine.localHostedSigning(canceled.id, 1000));
    const late = JSON.parse(r.engine.prepareJoinProof(raw, 1000));
    assert.throws(() => r.engine.localHostedSigning(late.id, 6000));
    const wrong = JSON.parse(r.engine.prepareJoinProof(raw, 1000));
    assert.throws(() => r.engine.finishHostedSigning(wrong.id, JSON.stringify({ receipt: wrong.work.receipt, values: [[1], [2]] }), 1000));
    assert.throws(() => r.engine.localHostedSigning(wrong.id, 1000));
  } finally { r.dispose(); }
});
