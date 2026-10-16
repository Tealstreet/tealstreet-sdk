// Extract and execute the original private utility and its actual Set declaration.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'TypeScript compiler path required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts');
const source = fs.readFileSync(file, 'utf8');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const selected = tree.statements.filter(node =>
  (ts.isFunctionDeclaration(node) && ['normalizeTickSize', 'parseGammaMarketTokens', 'precisionFromTickSize'].includes(node.name?.text)) ||
  (ts.isVariableStatement(node) && node.declarationList.declarations.some(d => d.name.getText(tree) === 'SUPPORTED_TICK_SIZES')));
assert.equal(selected.length, 4, 'actual_utility_declarations');
const original = selected.map(node => source.slice(node.getFullStart(), node.end)).join('\n');
const compiled = ts.transpileModule(original + '\nexports.normalizeTickSize = normalizeTickSize; exports.parseGammaMarketTokens = parseGammaMarketTokens; exports.precisionFromTickSize = precisionFromTickSize;', {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const context = { exports: {} };
vm.runInNewContext(compiled, context, { filename: file });
const numeric = ['0.1','0.01','0.005','0.0025','0.001','0.0001','-0','0','1','-1','0.10000000000000002','0.09999999999999999','1e400','-1e400','5e-324','1e-400','-1e-400','9007199254740993','1e21','1e-7'];
const strings = ['0.1','0.01','0.005','0.0025','0.001','0.0001','','0.10',' 0.1','0.1 ','1e-3','0.00010','0.002500','NaN','Infinity','missing','é','😀','\ud800','\udc00'];
const inputs = [...numeric.map(raw => ({kind:'number', raw})), ...strings.map(value => ({kind:'string', raw:JSON.stringify(value)})), {kind:'null',raw:'null'}];
const units = text => Array.from({length:text.length}, (_,index) => text.charCodeAt(index));
const cases = inputs.map(input => {
  const value = JSON.parse(input.raw);
  const item = {...input};
  if (input.kind === 'number') {
    const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value); item.bits = bytes.toString('hex');
  }
  try { item.output = context.exports.normalizeTickSize(value); }
  catch (error) { item.errorName = error.name; item.errorUnits = units(error.message); }
  return item;
});
const pairs = [
  [null, null], ['', '[]'], ['[]','[]'],
  ['["a"]','["Yes"]'], ['["a","b"]','["Yes","No"]'],
  ['["a","a"]','["Yes","No"]'], ['["a","b"]','["Yes"]'],
  ['["a"]','["Yes","No"]'], ['not-json','["Yes"]'],
  ['["a"]','not-json'], ['null','[]'], ['{}','[]'], ['"a"','[]'], ['42','[]'],
  ['[null,"a",false,"b",{},[],1]','["bad","Yes","bad","No","bad","bad","bad"]'],
  ['["a","b","c"]','["Yes",null,"No"]'],
  ['['+'['.repeat(256)+'0'+']'.repeat(256)+',"a"]','["bad","Yes"]'],
  [JSON.stringify(['é','e\u0301']),JSON.stringify(['😀','No'])],
  ['["", " a "]','["", " Yes "]'],
  [JSON.stringify(['\ud800','b']),JSON.stringify(['Yes','No'])],
  [JSON.stringify(['a','b']),JSON.stringify(['Yes','\udc00'])],
  ['[1e400,9007199254740993000000,"a"]','["bad","bad","Yes"]'],
];
const tokenCases = pairs.map(([tokenIds, outcomes]) => ({
  tokenIds, outcomes,
  output: context.exports.parseGammaMarketTokens({ clobTokenIds: tokenIds, outcomes })
    .map(value => ({ tokenId: units(value.tokenId), outcome: units(value.outcome) })),
}));
const precisionInputs = [...inputs, ...['0x10','0b11','0o10',' 0.0025 ','\ufeff0.001','1_000','1.23x','1e2','-0','+0','2.675','0.00500000'].map(value => ({kind:'string',raw:JSON.stringify(value)}))];
// All exact binary halfway values below 4, with their two adjacent floats.
for (let odd = 1; odd < 2000; odd += 2) {
  const buffer = Buffer.alloc(8); buffer.writeDoubleBE(odd / 512);
  const bits = buffer.readBigUInt64BE();
  for (const offset of [-1n, 0n, 1n]) {
    buffer.writeBigUInt64BE(bits + offset);
    precisionInputs.push({kind:'number',raw:JSON.stringify(buffer.readDoubleBE())});
  }
}
let state = 0x0123456789abcdefn;
for (let i = 0; i < 512; i++) {
  state = BigInt.asUintN(64, state * 6364136223846793005n + 1442695040888963407n);
  const buffer = Buffer.alloc(8); buffer.writeBigUInt64BE(state & 0x7fefffffffffffffn);
  precisionInputs.push({kind:'number',raw:JSON.stringify(buffer.readDoubleBE())});
}
const precisionCases = precisionInputs.map(input => {
  const value = JSON.parse(input.raw);
  const result = context.exports.precisionFromTickSize(value);
  const buffer = Buffer.alloc(8); buffer.writeDoubleBE(result);
  const item = {...input, outputBits:buffer.toString('hex')};
  if (input.kind === 'number') { buffer.writeDoubleBE(value); item.inputBits = buffer.toString('hex'); }
  return item;
});
process.stdout.write(JSON.stringify({capture:{architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0])},cases,tokenCases,precisionCases}));
