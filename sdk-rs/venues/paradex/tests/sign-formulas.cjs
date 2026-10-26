// Independent development oracle; run from the repository root with
// corepack yarn node sdk-rs/venues/paradex/tests/sign-formulas.cjs.
const fs = require('fs');
const { createRequire } = require('module');
const req = createRequire(process.cwd() + '/package.json');
const { hash, shortString } = req('starknet');
const { TypedDataEncoder, Wallet } = req('ethers');
const key = '11'.repeat(32);
const shortStrings = ['', 'Paradex', 'StarkNet Message', 'STARKNET_CONTRACT_ADDRESS', 'A'.repeat(31)]
  .map(input => ({ input, output: input ? shortString.encodeShortString(input) : '0x0' }));
const selectors = ['initialize', 'StarkNetDomain(name:felt,chainId:felt,version:felt)',
  'Request(method:felt,path:felt,body:felt,timestamp:felt,expiration:felt)',
  'Order(timestamp:felt,market:felt,side:felt,orderType:felt,size:felt,price:felt)',
  'ModifyOrder(timestamp:felt,market:felt,side:felt,orderType:felt,size:felt,price:felt,id:felt)']
  .map(input => ({ input, output: hash.getSelectorFromName(input) }));
const hashes = [[], ['0'], ['1', '2', '3'], ['0x800000000000011000000000000000000000000000000000000000000000000']]
  .map(input => ({ input, output: hash.computeHashOnElements(input) }));
const addresses = [
  { public: '0x1', proxy: '0x456', calldata: ['0x123', hash.getSelectorFromName('initialize'), '0x2', '0x1', '0x0'] },
  { public: '0x12345678901234567890', proxy: '0x789', calldata: [] },
  { public: '0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff', proxy: '0x123', calldata: ['0', '1', '2'] },
].map(row => ({ ...row, output: hash.calculateContractAddressFromHash(row.public, row.proxy, row.calldata, 0) }));
(async () => {
  const ethereum = [];
  for (const chain of ['0', '1', '11155111', '0x01', '0x' + 'f'.repeat(64)]) {
    const domain = { name: 'Paradex', version: '1', chainId: chain };
    const types = { Constant: [{ name: 'action', type: 'string' }] };
    const message = { action: 'STARK Key' };
    ethereum.push({ chain, key, digest: TypedDataEncoder.hash(domain, types, message),
      seed: await new Wallet('0x' + key).signTypedData(domain, types, message) });
  }
  fs.writeFileSync('sdk-rs/venues/paradex/tests/sign-formulas.json', JSON.stringify({
    oracle: 'Installed Starknet JS shortString/hash and ethers TypedDataEncoder.hash/Wallet.signTypedData; synthetic keys only. Reproduce with sign-formulas.cjs.',
    shortStrings, selectors, hashes, addresses, ethereum,
  }, null, 2) + '\n');
})();
