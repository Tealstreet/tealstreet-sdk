// Execute the actual vendored GRVT signing schema and Signer, offline only.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict'), {createRequire} = require('node:module');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const load = (relative) => {
  const file = path.join(root, relative);
  const source = fs.readFileSync(file, 'utf8');
  const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  assert.equal(tree.parseDiagnostics.length, 0, `${relative} source syntax`);
  const compiled = ts.transpileModule(source, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics:true});
  assert.equal(compiled.diagnostics.length, 0);
  const context = {exports:{},require:createRequire(file),Buffer};
  vm.runInNewContext(compiled.outputText,context,{filename:file});
  return context.exports;
};
const schema = load('thirdparty/grvt-ts-sdk/src/signing/types.ts');
const {Signer} = load('thirdparty/grvt-ts-sdk/src/signing/signer.ts');
const template = schema.OrderWithBuilderFee;
assert.equal(template.primaryType, 'OrderWithBuilderFee');
assert.equal(template.types.OrderWithBuilderFee.length, 10);
assert.equal(template.types.OrderLeg.length, 4);
const key = '0x' + '01'.repeat(32); // Public offline test key.
const cases = [false,true].flatMap(large=>[false,true].flatMap(isMarket=>[325,326].map(chainId=>{
  const typed = {...template,domain:{...schema.domain,chainId},message:{
    subAccountID:42,isMarket,timeInForce:1,postOnly:false,reduceOnly:true,
    legs:[{assetID:'0x030501',contractSize:large?9007199254740994:5000,limitPrice:isMarket?0:(large?1000000000000000100:50000000000000),isBuyingContract:true}],
    builder:'0xee8f3559f298fffa79b8253437969d15b5dccff3',builderFee:300,
    nonce:7,expiration:'1700003600000000000',
  }};
  const signature = Signer.sign(key,typed);
  return {key,typed,signature,decoded:Signer.decode(signature)};
})));
assert.equal(cases.length,8);
const builders = [
  '0xee8f3559f298fffa79b8253437969d15b5dccff3',
  '0xEE8F3559F298FFFA79B8253437969D15B5DCCFF3',
  '0xEe8f3559F298fffa79b8253437969d15b5dccff3',
];
const expanded = cases.flatMap(original => builders.map(builder => {
  const typed = {...original.typed, message: {...original.typed.message, builder}};
  const signature = Signer.sign(original.key, typed);
  assert.equal(signature, original.signature, 'SDK address casing does not change signed bytes');
  return {key:original.key, typed, signature, decoded:Signer.decode(signature)};
}));
assert.equal(expanded.length, 24, 'three address spellings for every native signing configuration');
process.stdout.write(JSON.stringify({template,cases:expanded}));
