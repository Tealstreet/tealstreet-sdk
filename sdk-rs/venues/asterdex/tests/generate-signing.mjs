// Independent ethers oracle; public documentation's demonstration key only.
// Run with: corepack yarn exec tsx sdk-rs/venues/asterdex/tests/generate-signing.mjs
import { writeFileSync } from 'node:fs';
import { createHmac } from 'node:crypto';
import { AbiCoder, getAddress, getBytes, hashMessage, keccak256, SigningKey, Signature, TypedDataEncoder } from 'ethers';
import axios from 'axios';

const privateKey = '0x4fd0a42218f3eae43a6ce26d22544e986139a01e5b34a62db53757ffca81bae1';
const user = getAddress('0x63DD5aCC6b1aa0f563956C0e534DD30B6dcF7C4e');
const signer = getAddress('0x21cF8Ae13Bb72632562c6Fff438652Ba1a151bb0');
const key = new SigningKey(privateKey);
const cases = [];
const query = (params) => Object.entries(params).filter(([, v]) => v !== undefined && v !== null && v !== '').map(([k, v]) => `${k}=${String(v)}`).join('&');
const sign = (digest) => Signature.from(key.sign(digest)).serialized;
for (const [name, params] of [
  ['documented-order', { symbol: 'ASTERUSDT', type: 'LIMIT', side: 'BUY', timeInForce: 'GTC', quantity: '20', price: '0.5' }],
  ['decimal-spelling', { symbol: 'BTCUSDT', quantity: '0.00010000', price: '92000.000', reduceOnly: 'false', note: 'quotes" slash\\ newline\n 雪 😀\u0000' }],
  ['query-encoding', { symbol: 'BTCUSDT', note: 'a b&c=d+$:,[]/!\'()*~' }],
  ['empty', {}],
]) {
  const nowMs = 1748310859508;
  const nonce = '1748310859508867';
  const standard = { ...params, recvWindow: 20000, timestamp: nowMs };
  const sorted = Object.fromEntries(Object.keys(standard).sort().map((k) => [k, String(standard[k])]));
  const payload = JSON.stringify(sorted);
  const abi = AbiCoder.defaultAbiCoder().encode(['string', 'address', 'address', 'uint256'], [payload, user, signer, BigInt(nonce)]);
  const hash = hashMessage(getBytes(keccak256(abi)));
  const v1Signature = createHmac('sha256', 'documentation-secret').update(query(standard)).digest('hex');
  const v1Query = axios.getUri({ url: 'https://unused.invalid', params: { ...standard, signature: v1Signature } }).split('?')[1];
  for (const testnet of [false, true]) {
    const codeQuery = query({ ...params, asterChain: testnet ? 'Testnet' : 'Mainnet', user, signer, nonce });
    const domain = { name: 'AsterSignTransaction', version: '1', chainId: testnet ? 714 : 1666, verifyingContract: '0x0000000000000000000000000000000000000000' };
    const codeHash = TypedDataEncoder.hash(domain, { Message: [{ name: 'msg', type: 'string' }] }, { msg: codeQuery });
    cases.push({ name: `${name}-${testnet ? 'testnet' : 'mainnet'}`, privateKey, user, signer, params, nowMs, nonce, payload, abi, hash, signature: sign(hash), codeQuery, codeHash, codeSignature: sign(codeHash), testnet, v1Query });
  }
}
writeFileSync(new URL('./fixtures/signing.json', import.meta.url), `${JSON.stringify({ source: 'ethers 6 and axios independent oracle; public Aster V3 documentation demonstration identities', cases }, null, 2)}\n`);
