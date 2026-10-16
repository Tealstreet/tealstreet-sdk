const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function execute(relative, imports = {}) {
  const filename = path.join(root, relative);
  const source = fs.readFileSync(filename, 'utf8');
  const compiled = ts.transpileModule(source, { compilerOptions: {
    module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022,
  }}).outputText;
  const module = { exports: {} };
  vm.runInThisContext(`(function(require,module,exports){${compiled}\n})`, {filename})(name => {
    assert(Object.hasOwn(imports, name), `unexpected dependency ${name}`);
    return imports[name];
  }, module, module.exports);
  return module.exports;
}
const shared = execute('packages/safe-cex/src/types.ts', {
  '@tealstreet/logging/src/LogSeverity': execute('packages/logging/src/LogSeverity.ts'),
});
const math = execute('packages/safe-cex/src/utils/safe-math.ts', {
  '@tealstreet/math/src/isNotNumber': execute('packages/math/src/isNotNumber.ts'),
});
const filename = path.join(root, 'packages/safe-cex/src/exchanges/phemex/phemex.exchange.ts');
const source = fs.readFileSync(filename, 'utf8');
const ast = ts.createSourceFile(filename, source, ts.ScriptTarget.ES2022, true);
const classes = ast.statements.filter(s => ts.isClassDeclaration(s) && s.name.text === 'PhemexExchange');
assert.equal(classes.length, 1, 'unique actual PhemexExchange');
function actualMethod(name, context = {}) {
  const members = classes[0].members.filter(m => m.name?.getText(ast) === name);
  assert.equal(members.length, 1, `unique actual ${name}`);
  const selected = ts.transpileModule(`const method = ${members[0].initializer.getText(ast)};`, {
    compilerOptions: {target: ts.ScriptTarget.ES2022}
  }).outputText;
  return vm.runInNewContext(`${selected}\nmethod`, {...context, subtract: math.subtract, PositionSide: shared.PositionSide});
}
const method = actualMethod('calculatePriceDiff');
function bits(value) {
  if (Number.isNaN(value)) return 'NaN';
  const b = Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex');
}
const values = [0, -0, 1, -1, 1.005, 0.1, 0.2, 1e-7, -1e-7, 1e20, 1e21, Number.MIN_VALUE,
  -Number.MIN_VALUE, Number.MAX_VALUE, Infinity, -Infinity, NaN, 9007199254740992, 3.141592653589793];
const cases = [];
for (const price of values) for (const mark of values) for (const side of [shared.PositionSide.Long, shared.PositionSide.Short]) {
  cases.push({price: bits(price), mark: bits(mark), side, result: bits(method(price, mark, side))});
}
assert.equal(cases.length, 722, 'price diff corpus census');

const symbolIn = actualMethod('formatSymbolIn');
const upper = [];
for (let cp = 0; cp <= 0x10ffff; cp++) {
  if (cp >= 0xd800 && cp <= 0xdfff) continue;
  const input = String.fromCodePoint(cp), result = symbolIn(input);
  if (input !== result) upper.push([cp, result]);
}
const strings = [undefined, null, '', 'btcusdt', 'BTCUSDT', 'ßςıﬃ', 'a\u0000z', '𐐨'];
const symbolInputs = strings.map(input => ({input: input ?? null, result: symbolIn(input)}));
assert.equal(symbolIn('\ud800').charCodeAt(0), 0xd800, 'source lone surrogate preserved');
const market = (symbol, info) => ({id: 'id-distinct', symbol, base:'BASE', quote:'QUOTE', active:true,
 precision:{amount:0.1,price:0.01}, limits:{amount:{min:1,max:2},leverage:{min:3,max:4}}, ...(info === undefined ? {} : {info})});
const outInputs = [
 {symbol:null, markets:null}, {symbol:'BTC', markets:null}, {symbol:'', markets:[]},
 {symbol:'BTC',markets:[market('OTHER',{symbol:'wrong'})]},
 {symbol:'BTC',markets:[market('BTC',{symbol:'first'}),market('BTC',{symbol:'second'})]},
 {symbol:'BTC',markets:[market('BTC',{symbol:''}),market('BTC',{symbol:'second'})]},
 ...[undefined,null,{},[],false,0,'text'].map(info=>({symbol:'BTC',markets:[market('BTC',info)]})),
 ...[null,false,0,-0,'',true,23,'btcusdt',[],{},['x']].map(value=>({symbol:'BTC',markets:[market('BTC',{symbol:value})]})),
 {symbol:'',markets:[market('',{symbol:'empty-match'})]},
 {symbol:null,markets:[market('',{symbol:'must-not-match'})]},
];
const symbolOutputs = outInputs.map(input => {
 const self = input.markets === null ? {} : {store:{markets:input.markets}};
 const result = actualMethod('formatSymbolOut', self)(input.symbol ?? undefined);
 return {...input,result};
});
// Numeric lexemes must reach Market.info unchanged, including nonfinite Number inputs.
const numericOutputs = ['1e400','-1e400','9007199254740993','-0','-1e-400','5e-324'].map(lexeme => {
 const raw = JSON.stringify(market('BTC',{symbol:'TOKEN'})).replace('"TOKEN"',lexeme);
 const result = actualMethod('formatSymbolOut',{store:{markets:[JSON.parse(raw)]}})('BTC');
 return {raw, type:typeof result, result:typeof result === 'number' ? bits(result) : result};
});
process.stdout.write(JSON.stringify({profile: {node: process.version, major: process.versions.node.split('.')[0], arch: process.arch, platform: process.platform}, cases, upper, symbolInputs, symbolOutputs, numericOutputs}));
