// Actual whole maintained TS module; expose only its private helper for direct evidence.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'compiler_required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const cache = new Map();
function execute(file) {
  if (cache.has(file)) return cache.get(file);
  const exports = {};
  cache.set(file, exports);
  let source = fs.readFileSync(file, 'utf8');

  const output = ts.transpileModule(source, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
  vm.runInNewContext(output, {exports, require(spec) {
    if (spec.startsWith('@tealstreet/')) return execute(path.join(root, 'packages', spec.slice('@tealstreet/'.length) + '.ts'));
    assert(spec.startsWith('.'), `unexpected_dependency:${spec}`);
    const target = path.resolve(path.dirname(file), spec);
    return execute(fs.existsSync(target + '.ts') ? target + '.ts' : path.join(target, 'index.ts'));
  }}, {filename: file});
  return exports;
}
const file = path.join(root, 'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts');
const source = fs.readFileSync(file,'utf8');
const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const top = ['getNormalizedInstType','isBlofinLinearSwapMarket','isKnownNonSwapInstrument'];
const methods = ['parsePositivePrice','mapBlofinTriggerType','toBlofinTriggerType','isAlreadyGoneCancelRow','isAlgoOrder'];
const constants = ['BLOFIN_SUPPORTED_INST_TYPE','BLOFIN_SUPPORTED_CONTRACT_TYPE'];
const selected = new Map();
for (const statement of ast.statements) {
  if (ts.isVariableStatement(statement)) for (const declaration of statement.declarationList.declarations) {
    if (ts.isIdentifier(declaration.name) && [...top,...constants].includes(declaration.name.text)) selected.set(declaration.name.text, declaration.initializer);
  }
  if (ts.isClassDeclaration(statement) && statement.name.text === 'BlofinExchange') for (const member of statement.members) {
    if (member.name && ts.isIdentifier(member.name) && methods.includes(member.name.text)) {
      assert(ts.isPropertyDeclaration(member) && ts.isArrowFunction(member.initializer),'helper_shape');
      selected.set(member.name.text,member.initializer);
    }
  }
}
assert.equal(selected.size,top.length+methods.length+constants.length,'helper_source_census');
let extracted = '';
for (const name of [...constants,...top,...methods]) {
  const expression = selected.get(name);
  assert(expression,'helper_source_exists:'+name);
  function guard(node) { assert(node.kind !== ts.SyntaxKind.ThisKeyword,'stateless_helper_requires_no_this'); ts.forEachChild(node,guard); }
  guard(expression);
  extracted += `const ${name} = ${expression.getText(ast)};\n`;
}
extracted += `Object.assign(exports, {${[...top,...methods].join(',')}});`;
const api = {};
const enums = execute(path.join(root,'packages/safe-cex/src/types.ts'));
vm.runInNewContext(ts.transpileModule(extracted,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText, {exports:api,...enums}, {filename:file});
function observe(value) {
  if (value === undefined) return {undefined:true};
  if (typeof value === 'number') { const b=Buffer.alloc(8); b.writeDoubleBE(Number.isNaN(value)?NaN:value); return {number:Number.isNaN(value)?'nan':b.toString('hex')}; }
  return value;
}
const values = ['null','true','false','0','-0','1e400','-1e400','-1e-400','9007199254740993','1.25','[]','[null]','["SWAP"]','["2"]','{}','{"toString":null}'];
for (const v of ['', 'SWAP','swap','ſwap','SPOT','linear','LINEAR','inverse','last','mark','index','LAST','mark_price','index_price','last_price','stop_market','take_profit_market','trailing_stop_market','market','limit','conditional','1000','prefix FILLED, TRIGGERED, CANCELED OR DOES NOT EXIST suffix','filled, triggered, canceled or doeſ not exist','0x10','1e2tail','1e2','-2','0','-0','+Infinity','NaN','\ufeff2','\u00852','雪😀','ßİΣςσ']) values.push(JSON.stringify(v));
const cases=[];
function run(name, raw) {
  let result;
  try { result={ok:observe(api[name](raw===undefined?undefined:JSON.parse(raw)))}; }
  catch(error) { result={error:error.name,message:error.message}; }
  cases.push({name,raw:raw??null,result});
}
for (const name of methods) for(const raw of [undefined,...values]) run(name,raw);
for(const a of values) for(const b of ['null','"linear"','"LINEAR"','"inverse"','{"toString":null}']) {
  const raw=`{"instType":${a},"contractType":${b}}`;
  for(const name of top) run(name,raw);
}
for(const name of top) for(const raw of ['null','[]','{}','true','"string"']) run(name,raw);
for(const code of ['null','1000','"1000"','"x"']) for(const msg of values) run('isAlreadyGoneCancelRow',`{"code":${code},"msg":${msg}}`);
const boundaries=['"\\ud800"','{"instType":"\\ud800"}','['.repeat(140)+'0'+']'.repeat(140)];
assert.equal(api.mapBlofinTriggerType(JSON.parse(boundaries[0])),undefined,'source_utility_surrogate');
assert.equal(api.getNormalizedInstType(JSON.parse(boundaries[1])).charCodeAt(0),0xd800,'source_utility_surrogate_unit');
assert.equal(api.parsePositivePrice(JSON.parse(boundaries[2])),undefined,'source_utility_deep');
process.stdout.write(JSON.stringify({ header: {architecture: process.arch, nodeMajor: Number(process.versions.node.split('.')[0]), nodeVersion: process.version}, cases,boundaries}));
