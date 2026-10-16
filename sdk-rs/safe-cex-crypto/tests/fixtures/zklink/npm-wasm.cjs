// Immutable oracle: installed npm zklink-web-sdk-alpha@1.0.2 WASM, never Rust output.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const {createHash, webcrypto} = require('node:crypto');
const root = path.resolve(__dirname, '../../../../../');
const { verifyPackage } = require('../oracle-packages.cjs');
const artifact = verifyPackage('zklink-web-sdk-alpha');
const hash = x => createHash('sha256').update(x).digest('hex');
async function main() {
  globalThis.crypto = webcrypto;
  const js = fs.readFileSync(path.join(artifact, 'zklink-sdk-web.js'));
  const wasm = fs.readFileSync(path.join(artifact, 'zklink-sdk-web_bg.wasm'));
  assert.equal(hash(js),'d20354a3ea5f13321312b36a0b6d5fc4e13eed71c3eda3ba1b2404c8b7db560e','zkLink oracle JS changed');
  assert.equal(hash(wasm),'4fc3cd426eef58022fb7228a8fcd6e268f6f243af36c848a55cabcbd1ab96c7e','zkLink oracle WASM changed');
  const sdk = await import('data:text/javascript;base64,' + js.toString('base64'));
  sdk.initSync(wasm);
  const cases = [];
  for (const [name, keyByte, nonce, pair] of [['base',1,1,1],['mutated-key',2,1,1],['mutated-payload',1,1,2],['mutated-nonce',1,2,1]]) {
    const seed = Buffer.concat([Buffer.alloc(64,keyByte),Buffer.from([27])]);
    const signer = sdk.newRpcSignerWithProvider({});
    await signer.initZklinkSigner('0x'+seed.toString('hex'));
    const contract = sdk.newContract(new sdk.ContractBuilder(1,0,1,nonce,pair,'0','0',true,1,2,false));
    const signed = signer.createSignedContract(contract);
    // Official Contract::get_bytes at pinned upstream: type/account/subaccount/
    // slot/24-bit nonce/pair/direction/zero 40-bit amount/zero 120-bit price/fees/subsidy.
    const payload = Buffer.alloc(36); payload[0]=0xfe; payload.writeUInt32BE(1,1);
    payload.writeUInt16BE(1,6); payload.writeUIntBE(nonce,8,3); payload[11]=pair;
    payload[12]=1; payload[34]=2; payload[33]=1;
    cases.push({name, seed:seed.toString('hex'), payload:payload.toString('hex'), nonce, pair, signed});
    signer.free();
  }
  const fixture=path.join(__dirname,'npm-wasm.json');
  const result=JSON.stringify({oracle:'zklink-web-sdk-alpha@1.0.2',jsSha256:hash(js),wasmSha256:hash(wasm),cases},null,2)+'\n';
  if(process.argv.includes('--capture'))fs.writeFileSync(fixture,result,{flag:'wx'});
  else assert.equal(result,fs.readFileSync(fixture,'utf8'),'zkLink WASM oracle drift');
}
main().catch(e=>{console.error(e);process.exitCode=1});
