// Self-authored field-identity supplement. The oracle is the pinned npm WASM,
// not native output. Existing feeder fixtures are never overwritten.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const {createHash, webcrypto} = require('node:crypto');
const vm = require('node:vm');
const {verifyPackage} = require('../oracle-packages.cjs');
const artifact = verifyPackage('zklink-web-sdk-alpha');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
async function main() {
  globalThis.crypto = webcrypto;
  const js = fs.readFileSync(path.join(artifact, 'zklink-sdk-web.js'));
  const wasm = fs.readFileSync(path.join(artifact, 'zklink-sdk-web_bg.wasm'));
  assert.equal(hash(js), 'd20354a3ea5f13321312b36a0b6d5fc4e13eed71c3eda3ba1b2404c8b7db560e');
  assert.equal(hash(wasm), '4fc3cd426eef58022fb7228a8fcd6e268f6f243af36c848a55cabcbd1ab96c7e');
  const sdk = await import('data:text/javascript;base64,' + js.toString('base64'));
  sdk.initSync(wasm);
  // Execute the current venue helper with the real SDK too. Only its module
  // imports are provided by this harness; its assertions and body are source.
  const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
  const sourceRoot = path.resolve(__dirname,
    '../../../../../packages/safe-cex/src/exchanges/apexomni/zklink/signer');
  const source = ['utils.ts', 'transactions/contract.ts'].map(file => {
    const text = fs.readFileSync(path.join(sourceRoot, file), 'utf8');
    const tree = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
    return tree.statements.filter(node => ts.isFunctionDeclaration(node)
      || ts.isVariableStatement(node)).map(node => node.getText(tree)).join('\n');
  }).join('\n');
  const compiled = ts.transpileModule(source, {compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText;
  const venue = new vm.Script(compiled + '\n({signContract})')
    .runInNewContext({exports: {}, Array, sdk});
  const base = {accountId: 16909060, subAccountId: 5, slotId: 1543,
    nonce: 526602, pairId: 11, size: '1234567890123456789',
    price: '987654321098765432109876543210', direction: true,
    feeRates: [12, 13], hasSubsidy: false};
  const variants = [['base', {}], ['account', {accountId: 16909061}],
    ['subaccount', {subAccountId: 6}], ['slot', {slotId: 1544}],
    ['nonce', {nonce: 526603}], ['pair', {pairId: 12}],
    ['size', {size: '2234567890123456789'}],
    ['price', {price: '987654321098765432109876543211'}],
    ['direction', {direction: false}], ['maker-fee', {feeRates: [14, 13]}],
    ['taker-fee', {feeRates: [12, 15]}], ['subsidy', {hasSubsidy: true}],
    ['wide-fields', {accountId: 4294967295, subAccountId: 255,
      slotId: 4294903303, nonce: 4278716682, pairId: 65291, feeRates: [254, 255]}],
    ['zero', {size: '0', price: '0'}],
    ['mantissa-max', {size: '34359738367'}],
    ['previous-exponent-nearer', {size: '34359738368'}],
    ['next-exponent-nearer', {size: '34359738379'}],
    ['maximum-safe-size', {size: '34359738367000000000000000000000000000'}],
    ['maximum-price', {price: '1329227995784915872903807060280344575'}],
    ['decimal-spelling', {size: '+0001_234_', price: '00056_78'}]];
  const cases = [];
  for (const [name, fields] of variants) {
    const input = {...base, ...fields};
    const seed = Buffer.concat([Buffer.alloc(64, 1), Buffer.from([27])]);
    const signer = sdk.newRpcSignerWithProvider({});
    await signer.initZklinkSigner('0x' + seed.toString('hex'));
    const c = input;
    const contract = sdk.newContract(new sdk.ContractBuilder(c.accountId,
      c.subAccountId, c.slotId, c.nonce, c.pairId, c.size, c.price,
      c.direction, c.feeRates[0], c.feeRates[1], c.hasSubsidy));
    const signed = signer.createSignedContract(contract);
    assert.equal(JSON.stringify((await venue.signContract(signer, input)).tx),
      JSON.stringify(signed), `actual TS contract orchestration: ${name}`);
    cases.push({name, input, seed: seed.toString('hex'), signed});
    signer.free();
  }
  // Initialization consumes all 65 bytes, including v; comments about Ethereum
  // recovery normalization must not silently change this actual seed boundary.
  for (const v of [0, 1, 28, 255]) {
    const seed = Buffer.concat([Buffer.alloc(64, 2), Buffer.from([v])]);
    const signer = sdk.newRpcSignerWithProvider({});
    await signer.initZklinkSigner(seed.toString('hex'));
    const c = base;
    const contract = sdk.newContract(new sdk.ContractBuilder(c.accountId,
      c.subAccountId, c.slotId, c.nonce, c.pairId, c.size, c.price,
      c.direction, c.feeRates[0], c.feeRates[1], c.hasSubsidy));
    const signed = signer.createSignedContract(contract);
    assert.equal(JSON.stringify((await venue.signContract(signer, c)).tx),
      JSON.stringify(signed), `actual TS contract orchestration: seed-v-${v}`);
    cases.push({name: `seed-v-${v}`, input: c, seed: seed.toString('hex'), signed});
    signer.free();
  }
  const amounts = ['0', '1', '2047', '2048', '2050', '34359738367',
    '34359738368', '34359738379', '1234567890123456789',
    '34359738367000000000000000000000000000'].map(input =>
      ({input, output: sdk.closestPackableTransactionAmount(input)}));
  const fees = ['0', '1', '2047', '2048', '2050', '2059',
    '1234567890123456789', '20470000000000000000000000000000000'].map(input =>
      ({input, output: sdk.closestPackableTransactionFee(input)}));
  const result = JSON.stringify({oracle: 'zklink-web-sdk-alpha@1.0.2',
    source: 'zkLinkProtocol/zklink_sdk@3152525e2615ecbbdb56fd6728ce265ad2dde485',
    note: 'Self-authored ContractBuilder field supplement; npm artifact and pinned source are separate references, not an identical-build claim.',
    jsSha256: hash(js), wasmSha256: hash(wasm), cases, amounts, fees}, null, 2) + '\n';
  const fixture = path.join(__dirname, 'contracts.json');
  if (process.argv.includes('--capture')) fs.writeFileSync(fixture, result, {flag: 'wx'});
  else assert.equal(result, fs.readFileSync(fixture, 'utf8'), 'zkLink contract oracle drift');
}
main().catch(error => {console.error(error); process.exitCode = 1;});
