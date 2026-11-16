// Run against the optimized Hyperliquid family (both aliases), without venue IO.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';
import { TypedDataEncoder, recoverAddress } from 'ethers';
import { EngineHost } from './host.mjs';

const { WasmEngine } = createRequire(import.meta.url)('../pkg/node/safe_cex_wasm.js');
const vectors = JSON.parse(readFileSync(new URL('../../../crypto/tests/fixtures/join-proof.json', import.meta.url)));
const domain = { name: 'Tealstreet Admission', version: '1' };
const types = { Admission: [{ name: 'challenge', type: 'bytes32' }] };

for (const venue of ['hyperliquid', 'hyperliquidspot']) {
  const vector = vectors.cases.find(item => item.facts.venue === venue);
  const request = () => ({ id: 'captured-attempt', type: 'join-proof', exchangeName: venue,
    accountName: 'account', instanceId: 'registration',
    joinProof: { version: 1, statement: structuredClone(vector.statement) } });
  function rig() {
    const scalar = Buffer.alloc(32); scalar[31] = 1;
    const engine = new WasmEngine(venue, JSON.stringify({ key: vector.facts.credential.signer }),
      JSON.stringify({ primary: { secret: scalar.toString('hex'), secretEncoding: 'utf8' } }));
    const host = new EngineHost(engine, { now: () => 1000, setTimeout, clearTimeout,
      fetch() { throw new Error('proof cannot perform HTTP'); },
      openSocket() { throw new Error('proof cannot open sockets'); },
    });
    return { engine, host, dispose() { host.dispose(); engine.free(); } };
  }

  test(`${venue} actual WASM signs the admission domain with its owned scalar, independently recovered by ethers`, async () => {
    const r = rig();
    try {
      const response = await r.host.hostedJoinProof(JSON.stringify(request()));
      assert.deepEqual(Object.keys(response).sort(), ['id', 'joinProof']);
      assert.equal(response.id, 'captured-attempt');
      assert.equal(response.joinProof.version, 1);
      const signature = `0x${Buffer.from(response.joinProof.signature, 'base64').toString('hex')}`;
      const digest = TypedDataEncoder.hash(domain, types, { challenge: `0x${vector.challengeDigest}` });
      assert.equal(recoverAddress(digest, signature).toLowerCase(), vector.facts.credential.signer);
      const transactionDomain = TypedDataEncoder.hash({ name: 'Exchange', version: '1', chainId: 1337,
        verifyingContract: `0x${'00'.repeat(20)}` }, { Agent: [{ name: 'source', type: 'string' },
        { name: 'connectionId', type: 'bytes32' }] }, { source: 'a', connectionId: `0x${vector.challengeDigest}` });
      assert.notEqual(recoverAddress(transactionDomain, signature).toLowerCase(), vector.facts.credential.signer);
      assert.equal(r.host.started, false);
      assert.equal(r.host.hostedSignings.size, 0);
    } finally { r.dispose(); }
  });

  test(`${venue} actual WASM refuses foreign, duplicate and transaction-shaped requests before signing`, async () => {
    const r = rig();
    try {
      const raw = JSON.stringify(request());
      for (const invalid of [raw.replace('"version":1', '"version":1,"version":1'),
        raw.replace(`"exchangeName":"${venue}"`, `"exchangeName":"${venue === 'hyperliquid' ? 'hyperliquidspot' : 'hyperliquid'}"`),
        raw.replace('"joinProof":', '"digest":"caller-owned","joinProof":'),
        '{"kind":"http","payload":"withdraw"}', '["join-proof","caller-digest"]']) {
        await assert.rejects(r.host.hostedJoinProof(invalid));
        assert.equal(r.host.hostedSignings.size, 0);
      }
      const pending = JSON.parse(r.engine.prepareJoinProof(raw, 1000));
      r.engine.cancelHostedSigning(pending.id);
      assert.throws(() => r.engine.localHostedSigning(pending.id, 1000));
      const late = JSON.parse(r.engine.prepareJoinProof(raw, 1000));
      assert.throws(() => r.engine.localHostedSigning(late.id, 6000));
    } finally { r.dispose(); }
  });
}
