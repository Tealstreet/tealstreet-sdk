// Independent Node/OpenSSL Ed25519 oracle; never imports Rust output.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fixture = path.join(__dirname, 'node.json');
const cases = [];
for (const [i, payload] of ['', '{"zeta":"雪\\u0000","alpha":1,"nonce":"7"}', '{"zeta":"transfer","alpha":2,"nonce":"8"}'].entries()) {
  const seed = Buffer.alloc(32, i + 1);
  const privateKey = crypto.createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed]), format: 'der', type: 'pkcs8' });
  const publicKeyObject = crypto.createPublicKey(privateKey);
  const publicKey = publicKeyObject.export({format:'der', type:'spki'}).subarray(-32).toString('hex');
  const signature = crypto.sign(null, Buffer.from(payload), privateKey).toString('hex');
  const changedSeed = Buffer.from(seed); changedSeed[0] ^= 1;
  const changedPrivateKey = crypto.createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), changedSeed]), format: 'der', type: 'pkcs8' });
  assert.equal(crypto.verify(null, Buffer.from(payload), crypto.createPublicKey(changedPrivateKey), Buffer.from(signature,'hex')), false, 'mutated key must reject');
  const changedPayload = payload + ' ';
  assert.equal(crypto.verify(null, Buffer.from(payload), publicKeyObject, Buffer.from(signature,'hex')), true);
  assert.equal(crypto.verify(null, Buffer.from(changedPayload), publicKeyObject, Buffer.from(signature,'hex')), false);
  const noncePayload = payload.includes('"nonce"') ? payload.replace(/"nonce":"[78]"/, '"nonce":"9"') : 'nonce=9';
  assert.equal(crypto.verify(null, Buffer.from(noncePayload), publicKeyObject, Buffer.from(signature,'hex')), false);
  cases.push({ seed: seed.toString('hex'), publicKey, payload, signature, changedPayload, noncePayload });
}
const result = JSON.stringify({ oracle:'Node crypto/OpenSSL Ed25519, PKCS8 32-byte seeds; deterministic signing has no caller nonce. noncePayload changes application bytes.', cases }, null, 2) + '\n';
if (process.argv.includes('--capture')) fs.writeFileSync(fixture, result, {flag:'wx'});
else assert.equal(result, fs.readFileSync(fixture,'utf8'), 'Ed25519 oracle drift');
console.log('Ed25519 oracle and payload/nonce controls passed');
