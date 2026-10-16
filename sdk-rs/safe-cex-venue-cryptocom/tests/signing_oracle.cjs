// Actual source, unchanged, evaluated once for the whole corpus.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'compiler_path_required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/cryptocom/cryptocom.signing.ts');
const source = fs.readFileSync(file, 'utf8');
const compiled = ts.transpileModule(source, {
  fileName: file, reportDiagnostics: true,
  compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022},
});
assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0, 'signing_transpile');
const context = vm.createContext({exports: {}, require: name => {
  // These collaborators must never execute in a paramsToString-only slice.
  if (name === 'create-hmac') return () => { throw Error('unexpected_hmac_call'); };
  if (name === '../../utils/virtualClock') return {virtualClock: {getCurrentTimeMs() { throw Error('unexpected_clock_call'); }}};
  throw Error(`unexpected_import:${name}`);
}});
vm.runInContext(compiled.outputText + '\nexports.__paramsToString = paramsToString;', context, {filename: file});
const run = context.exports.__paramsToString;
assert.equal(typeof run, 'function', 'params_source_admission');
const inputs = [
  ['missing', null], ['null', 'null'], ['false', 'false'], ['true', 'true'],
  ['empty_string', '""'], ['unicode', '"A🙂é"'], ['negative_zero', '-0'],
  ['rounded_integer', '9007199254740993'], ['overflow', '1e400'], ['negative_overflow', '-1e400'],
  ['subnormal', '5e-324'], ['fixed', '0.000001'], ['exponential', '1e-7'],
  ['empty_array', '[]'], ['array', '["first",null,2,"last"]'],
  ['nested_array', '[["left",null],["right",[1,2]]]'], ['empty_object', '{}'],
  ['sorted_keys', '{"z":"last","a":"first","m":3}'],
  ['integer_keys', '{"2":"two","10":"ten","01":"zero-one"}'],
  ['utf16_keys', '{"\ue000":"bmp","🙂":"pair","a":"ascii"}'],
  ['prototype_keys', '{"__proto__":"own","constructor":2,"toString":"shadow"}'],
  ['depth_object', '{"a":{"b":{"c":{"d":"deep"}}}}'],
  ['depth_array', '{"a":{"b":["left",null,[1,2],{"k":3}]}}'],
  ['coercion_error', '{"a":{"b":{"toString":0}}}'],
  ['array_coercion_error', '[[[{"toString":false}]]]'],
  ['duplicates', '{"a":"first","a":"last","b":"other"}'],
  ['empty_key', '{"":"empty-key","b":null}'],
  ['number_array', '[-0,1e400,9007199254740993]'],
];
const levels = [null, '-1', '0', '2', '3', '4', 'NaN', 'Infinity', '-Infinity', '2.5'];
const rows = [];
for (const [name, raw] of inputs) for (const level of levels) {
  const value = raw === null ? undefined : JSON.parse(raw);
  const numericLevel = level === null ? undefined : Number(level);
  let result;
  try { result = {value: run(value, numericLevel)}; }
  catch (e) { result = {error: [e.name, e.message]}; }
  rows.push({name, raw, level, ...result});
}
// Excluded-domain evidence is kept distinct from mapped JSON cases.
const excluded = vm.runInContext(`(() => {
  let calls = 0;
  const getter = {get a() { calls++; return 'getter'; }};
  const out = {
    nestedUndefined: paramsToString({a: undefined}),
    bigint: paramsToString(123n),
    symbol: paramsToString(Symbol('x')),
    hole: paramsToString([,'last']),
    getter: paramsToString(getter),
    customPrototype: paramsToString(Object.create({inherited: 1}, {own: {value: 'data', enumerable: true}})),
    callable: paramsToString(function witness() {}),
    surrogateCode: paramsToString('\\ud800').charCodeAt(0),
  };
  out.getterCalls = calls;
  return out;
})()`, context);
assert.equal(rows.length, 280, 'source_params_total');
console.log(JSON.stringify({header: {node:process.version, nodeMajor:Number(process.versions.node.split('.')[0]), architecture:process.arch, platform:process.platform}, rows, excluded}));
