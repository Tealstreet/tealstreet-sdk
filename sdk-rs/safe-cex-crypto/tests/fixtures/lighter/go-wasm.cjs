// Oracle: execute the checked-in Go WASM unchanged; never import Rust output.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { createHash } = require('node:crypto');
const root = path.resolve(__dirname, '../../../../../');
const source = path.join(root, 'packages/poseidon-crypto/src/wasm');
const twoNoncanonicalMode = process.argv.includes('--two-noncanonical');
const lengthsMode = process.argv.includes('--lengths');
const securityMode = process.argv.includes('--security');
const edgeMode = process.argv.includes('--edges');
const noncanonicalMode = process.argv.includes('--noncanonical');
const fixture = path.join(__dirname, twoNoncanonicalMode ? 'go-wasm-two-noncanonical.json' : lengthsMode ? 'go-wasm-lengths.json' : securityMode ? 'go-wasm-security.json' : noncanonicalMode ? 'go-wasm-noncanonical.json' : edgeMode ? 'go-wasm-edges.json' : 'go-wasm.json');
let nonce = 7;
let entropy = [];
Object.defineProperty(globalThis, 'crypto', { value: { getRandomValues(bytes) {
  bytes.fill(0); bytes[bytes.length - 1] = nonce;
  entropy.push(Buffer.from(bytes).toString('hex'));
  return bytes;
} }, configurable: true });
require(path.join(source, 'wasm_exec.js'));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const word = value => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(value)); return b.toString('hex'); };
const must = result => { assert.equal(typeof result, 'string', JSON.stringify(result)); return result; };
async function main() {
  const wasm = fs.readFileSync(path.join(source, 'poseidon_crypto.wasm'));
  const go = new Go();
  const { instance } = await WebAssembly.instantiate(wasm, go.importObject);
  go.run(instance).catch(error => { console.error(error); process.exit(1); });
  assert.equal(typeof globalThis.schnorrSign, 'function');
  if (twoNoncanonicalMode) {
    const canonical=word(1).repeat(4), noncanonical=word(0xffffffffffffffffn)+word(2).repeat(3);
    const twoToOne=[[canonical,canonical],[noncanonical,canonical],[canonical,noncanonical],[noncanonical,noncanonical]].map(([left,right])=>({left,right,hash:must(poseidon2HashTwoToOne(left,right))}));
    const result={provenance:{wasmSha256:sha256(wasm),runtimeSha256:sha256(fs.readFileSync(path.join(source,'wasm_exec.js')))},twoToOne};
    const serialized=JSON.stringify(result,null,2)+'\n';
    if(process.argv.includes('--capture'))fs.writeFileSync(fixture,serialized,{flag:'wx'});
    else assert.equal(serialized,fs.readFileSync(fixture,'utf8'),'Go WASM noncanonical two-to-one oracle drift');
    console.log('Go WASM canonical and noncanonical two-to-one vectors passed');process.exit(0);
  }
  if (lengthsMode) {
    const key='01'+'00'.repeat(39), publicKey=must(schnorrPkFromSk(key));
    nonce=7; const validSignature=must(schnorrSign(word(1),key));
    const invalidInputs=[4,9,15].map(length=>{
      const input=Buffer.alloc(length,1).toString('hex');
      const hash=poseidon2Hash(input), fp5=poseidon2HashFp5(input), signature=schnorrSign(input,key), verified=schnorrVerify(input,publicKey,validSignature);
      for(const [operation,result] of Object.entries({hash,fp5,signature,verified})) assert.ok(result && typeof result==='object' && typeof result.error==='string',`Go ${operation} rejects ${length} bytes`);
      return {input,hash,fp5,signature,verified};
    });
    const result={provenance:{wasmSha256:sha256(wasm),runtimeSha256:sha256(fs.readFileSync(path.join(source,'wasm_exec.js')))},key,publicKey,validSignature,invalidInputs};
    const serialized=JSON.stringify(result,null,2)+'\n';
    if(process.argv.includes('--capture'))fs.writeFileSync(fixture,serialized,{flag:'wx'});
    else assert.equal(serialized,fs.readFileSync(fixture,'utf8'),'Go WASM malformed-length oracle drift');
    console.log('Go WASM rejects 4/9/15-byte payloads in every raw-message operation');process.exit(0);
  }
  if (securityMode) {
    const signatures=[];
    for (const key of ['01'+'00'.repeat(39),'12345678'.repeat(9)+'00000000']) {
      const payload=word(1)+word(2), publicKey=must(schnorrPkFromSk(key));
      for (const value of [0,7]) {
        nonce=value;entropy=[];
        const signature=must(schnorrSign(payload,key));
        const verified=schnorrVerify(payload,publicKey,signature);
        assert.equal(verified,value!==0,'Go zero-nonce rejection/nonzero control');
        signatures.push({key,payload,publicKey,nonce:value.toString(16).padStart(2,'0')+'00'.repeat(39),entropy:entropy.slice(),signature,verified});
      }
    }
    const result={provenance:{wasmSha256:sha256(wasm),runtimeSha256:sha256(fs.readFileSync(path.join(source,'wasm_exec.js')))},signatures};
    const serialized=JSON.stringify(result,null,2)+'\n';
    if(process.argv.includes('--capture'))fs.writeFileSync(fixture,serialized,{flag:'wx'});
    else assert.equal(serialized,fs.readFileSync(fixture,'utf8'),'Go WASM nonce-security oracle drift');
    console.log('Go WASM zero-nonce rejection and nonzero controls passed');process.exit(0);
  }
  if (noncanonicalMode) {
    const inputs = [word(0xffffffff00000001n), word(0xffffffffffffffffn), word(1)+word(0xffffffffffffffffn), Buffer.from('{"z":12,"a":345}').toString('hex')];
    const hashes = inputs.map(input => ({input, hash:must(poseidon2Hash(input)), fp5:must(poseidon2HashFp5(input))}));
    const key = '01'+'00'.repeat(39), payload = inputs[3];
    nonce=13;entropy=[];
    const signature=must(schnorrSign(payload,key));
    const publicKey=must(schnorrPkFromSk(key));assert.equal(schnorrVerify(payload,publicKey,signature),true);
    const result={provenance:{wasmSha256:sha256(wasm),runtimeSha256:sha256(fs.readFileSync(path.join(source,'wasm_exec.js')))},hashes,
      signature:{key,publicKey,payload,nonce:'0d'+'00'.repeat(39),entropy,signature}};
    const serialized=JSON.stringify(result,null,2)+'\n';
    if(process.argv.includes('--capture'))fs.writeFileSync(fixture,serialized,{flag:'wx'});
    else assert.equal(serialized,fs.readFileSync(fixture,'utf8'),'Go WASM noncanonical oracle drift');
    console.log('Go WASM noncanonical and exact-body vectors passed');process.exit(0);
  }
  if (edgeMode) {
    const inputs = [Array.from({length:5}, (_,i) => word(i+1)).join(''), '00'.repeat(40)];
    const hashes = inputs.map(input => ({input, hash: must(poseidon2Hash(input)), fp5: must(poseidon2HashFp5(input))}));
    const keys = ['00'.repeat(40), 'ff'.repeat(40)].map(key => ({key, publicKey: must(schnorrPkFromSk(key))}));
    const key = keys[1].key, payload = inputs[0];
    nonce = 11; entropy = [];
    const signature = must(schnorrSign(payload, key));
    assert.equal(schnorrVerify(payload, keys[1].publicKey, signature), true);
    const result = { provenance: {wasmSha256:sha256(wasm), runtimeSha256:sha256(fs.readFileSync(path.join(source,'wasm_exec.js')))}, hashes, keys,
      signature: {key, publicKey:keys[1].publicKey, payload, nonce:'0b'+'00'.repeat(39), entropy, signature} };
    const serialized = JSON.stringify(result,null,2)+'\n';
    if(process.argv.includes('--capture')) fs.writeFileSync(fixture,serialized,{flag:'wx'});
    else assert.equal(serialized,fs.readFileSync(fixture,'utf8'),'Go WASM edge oracle drift');
    console.log('Go WASM five-element and scalar edge vectors passed');process.exit(0);
  }
  const hashes = [];
  for (const count of [0, 1, 4, 7, 8, 9, 12, 16, 17]) {
    const input = Array.from({ length: count }, (_, i) => word(i + 1)).join('');
    hashes.push({ input, hash: must(poseidon2Hash(input)), fp5: must(poseidon2HashFp5(input)) });
  }
  for (const input of [word(0), word(0xffffffff00000000n), word(1) + word(0xffffffff00000000n)]) {
    hashes.push({ input, hash: must(poseidon2Hash(input)), fp5: must(poseidon2HashFp5(input)) });
  }
  const twoToOne = hashes.slice(1, 4).map((a, i) => ({ left: a.hash, right: hashes[i + 4].hash,
    hash: must(poseidon2HashTwoToOne(a.hash, hashes[i + 4].hash)) }));
  const signatures = [];
  for (const [i, key] of ['01' + '00'.repeat(39), '02' + '00'.repeat(39), '12345678'.repeat(9) + '00000000'].entries()) {
    const payload = hashes[[1, 5, 8][i]].input;
    const publicKey = must(schnorrPkFromSk(key));
    nonce = 7 + i; entropy = [];
    const signature = must(schnorrSign(payload, key));
    const calls = entropy.slice();
    assert.deepEqual(calls, ['00'.repeat(39) + nonce.toString(16).padStart(2, '0')], 'Go scalar sampler entropy');
    assert.equal(schnorrVerify(payload, publicKey, signature), true, 'Go signature verification');
    const changedPayload = word(42) + payload.slice(16);
    const changedKey = '03' + '00'.repeat(39);
    const changedPublicKey = must(schnorrPkFromSk(changedKey));
    assert.equal(schnorrVerify(changedPayload, publicKey, signature), false, 'Go mutated payload');
    assert.equal(schnorrVerify(payload, changedPublicKey, signature), false, 'Go mutated key');
    nonce += 1;
    const changedNonceSignature = must(schnorrSign(payload, key));
    assert.notEqual(changedNonceSignature, signature, 'Go mutated nonce signature');
    assert.equal(schnorrVerify(payload, publicKey, changedNonceSignature), true, 'changed nonce remains valid');
    signatures.push({ key, publicKey, payload, nonce: (7 + i).toString(16).padStart(2, '0') + '00'.repeat(39),
      entropy: calls, signature, changedKey, changedPayload, changedNonceSignature });
  }
  const result = { provenance: { wasmSha256: sha256(wasm), runtimeSha256: sha256(fs.readFileSync(path.join(source, 'wasm_exec.js'))),
    entropy: 'getRandomValues: zero bytes followed by nonce; Go crypto/rand.Int reads big-endian scalar' }, hashes, twoToOne, signatures };
  const serialized = JSON.stringify(result, null, 2) + '\n';
  if (process.argv.includes('--capture')) fs.writeFileSync(fixture, serialized, { flag: 'wx' });
  else assert.equal(serialized, fs.readFileSync(fixture, 'utf8'), 'Go WASM oracle drift; do not adjust fixtures to fit Rust');
  console.log('Go WASM vectors and key/payload/nonce controls passed');
  process.exit(0);
}
main().catch(error => { console.error(error); process.exit(1); });
