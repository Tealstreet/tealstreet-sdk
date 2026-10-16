// Independent oracle: repository ethers, never the Rust implementation.
const { oracleRequire, verifyPackage } = require('../oracle-packages.cjs');
verifyPackage('ethers');
const { Wallet, TypedDataEncoder, Signature } = oracleRequire('ethers');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const key = '0x' + '11'.repeat(32);
const domain = { name: 'PrimitiveVector', version: '1', chainId: 1337, verifyingContract: '0x0000000000000000000000000000000000000000' };
const types = { Message: [{ name: 'payload', type: 'string' }, { name: 'nonce', type: 'uint64' }] };
async function main() {
  const cases = [];
  for (const [name, privateKey, payload, nonce] of [
    ['base', key, '雪\u0000payload', '7'],
    ['key', '0x' + '22'.repeat(32), '雪\u0000payload', '7'],
    ['payload', key, '雪\u0000changed', '7'],
    ['nonce', key, '雪\u0000payload', '8'],
    ['empty', key, '', '0'],
    ['uint64-max', key, 'maximum', '18446744073709551615'],
  ]) {
    const message = { payload, nonce };
    const typedData = TypedDataEncoder.getPayload(domain, types, message);
    const wallet = new Wallet(privateKey);
    const signature = await wallet.signTypedData(domain, types, message);
    const split = Signature.from(signature);
    cases.push({ name, privateKey, typedData: JSON.stringify(typedData), hash: TypedDataEncoder.hash(domain, types, message), signature, r: split.r, s: split.s, v: split.v, address: wallet.address.toLowerCase() });
  }
  const fixture = path.join(__dirname, 'vectors.json');
  const result = JSON.stringify({ oracle: 'ethers ' + oracleRequire('ethers').version, cases }, null, 2) + '\n';
  if (process.argv.includes('--capture')) fs.writeFileSync(fixture, result, {flag:'wx'});
  else assert.equal(result, fs.readFileSync(fixture,'utf8'), 'Ethereum oracle drift');
}
main().catch(error => { console.error(error); process.exitCode = 1; });
