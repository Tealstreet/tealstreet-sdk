const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/apexomni/apexomni.utils.ts');
let source = fs.readFileSync(file, 'utf8');
const mutations = {
  precision: ["strList[1].length", "strList[1].length + 1"],
  random: ["apexomni-tealstreet-", "apexomni-MUTATED-"],
  prefix: ["/^0x/", "/^0X/"],
};
if (process.env.APEXOMNI_UTILS_MUTATE_TS) {
  const [from, to] = mutations[process.env.APEXOMNI_UTILS_MUTATE_TS];
  assert.equal(source.split(from).length, 2, 'source mutation applies exactly once');
  source = source.replace(from, to);
}
const compiled = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});
assert(!compiled.diagnostics.some(d => d.category === ts.DiagnosticCategory.Error), 'transpilation succeeds');
let draw = 0, calls = 0;
const math = Object.create(Math);
math.random = () => {calls++; return draw;};
const moduleValue = {exports:{}};
new vm.Script(`(function(require,module,exports){${compiled.outputText}\n})`, {filename:file}).runInNewContext({Math:math})(
  name => {
    assert(['@noble/hashes/hmac.js','@noble/hashes/sha2.js','@noble/hashes/utils.js'].includes(name), `unexpected import ${name}`);
    // These three rows never read cryptographic imports. Any attempted use fails;
    // this corpus neither executes nor claims to witness the pending HMAC row.
    return new Proxy({}, {get(){throw Error('HMAC is outside the utility corpus');}});
  }, moduleValue, moduleValue.exports);
const api = moduleValue.exports;
function bits(value) {
  const bytes = Buffer.alloc(8); bytes.writeDoubleBE(Number.isNaN(value) ? NaN : value);
  return Number.isNaN(value) ? '7ff8000000000000' : bytes.toString('hex');
}
const strings = ['', ' ', '\ufeff1.2300', '1.2300 ', '.25', '1.', '1.2.3', '0x1f', '0b11', '0o71', 'NaN', 'Infinity', '-Infinity', '1.234e-7', '1.234e+400', '1.2e-4000', '-0.00', '1.2\n', '1.2😀', '１２.３', '1_000.0', '9'.repeat(320)+'.123'];
const numeric = ['0','-0','1.25','0.0000001','0.000001','1.234e-7','1e21','1e20','9007199254740993','1e400','-1e400'];
const precision = [...strings.map(value => JSON.stringify(value)), ...numeric].map(raw => {
  const value = JSON.parse(raw);
  return {raw,type:typeof value,bits:typeof value === 'number' ? bits(value) : null,output:api.getPrecision(value)};
});
const native = [NaN,-NaN,Infinity,-Infinity,-0,0,1.25,Number.MIN_VALUE].map(value => ({bits:bits(value),type:typeof value,output:api.getPrecision(value)}));
const draws = [0,-0,0.5,0.000123,1e-7,1e-6,Number.MIN_VALUE,0.9999999999999999,0.000001234567890123,0.0000001234567890123];
const random = draws.map(value => {draw=value;calls=0; const output=api.generateApeXRandomClientId();return {bits:bits(value),output,calls};});
const prefix = ['', '0x', '0x123', '0x0x123', '0X123', ' 0x123', 'a0x123', '0x😀', '0x\nabc', '😀0x'].map(input => ({input,output:api.dropPrefix(input)}));
console.log(JSON.stringify({header:{node:process.version,nodeMajor:Number(process.versions.node.split('.')[0]),nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH},precision,native,random,prefix}));
