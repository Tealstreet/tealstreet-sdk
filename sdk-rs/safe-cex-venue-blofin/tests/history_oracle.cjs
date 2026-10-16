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
  if (file.endsWith('blofin.order-history.ts')) source += '\nexport { toNumber, toOptionalNumber, ORDER_TYPE, ORDER_STATUS };';
  const output = ts.transpileModule(source, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
  vm.runInNewContext(output, {exports, require(spec) {
    if (spec.startsWith('@tealstreet/')) return execute(path.join(root, 'packages', spec.slice('@tealstreet/'.length) + '.ts'));
    assert(spec.startsWith('.'), `unexpected_dependency:${spec}`);
    const target = path.resolve(path.dirname(file), spec);
    return execute(fs.existsSync(target + '.ts') ? target + '.ts' : path.join(target, 'index.ts'));
  }}, {filename: file});
  return exports;
}
const api = execute(path.join(root, 'packages/safe-cex/src/exchanges/blofin/blofin.order-history.ts'));
function observe(value) {
  if (value === undefined) return {undefined: true};
  if (typeof value === 'function') return {inherited: 'constructor'};
  if (value === Object.getPrototypeOf(api.ORDER_TYPE)) return {inherited: '__proto__'};
  if (typeof value === 'number') {
    const b = Buffer.alloc(8); b.writeDoubleBE(Number.isNaN(value) ? NaN : value);
    return {number: Number.isNaN(value) ? 'nan' : b.toString('hex')};
  }
  if (Array.isArray(value)) return Array.from(value, observe);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v]) => [k,observe(v)]));
  return value;
}
const inputs = ['null','true','false','0','-0','1e400','-1e400','-1e-400','9007199254740993','1.25','[]','[null]','["-2.5tail"]','{}','{"toString":null}'];
for (const s of ['', ' ', '\ufeff-0junk', '\u008512', '1.25e2tail', '-3.75', '+Infinity', 'NaN', '0x10', '1e', '.5tail', '-5e-324', '1e309', '12😀', '雪', 'SHORT', 'TRUE', 'buy', 'SELL', 'net']) inputs.push(JSON.stringify(s));
const cases = [];
function run(op, raw, resolved = 'resolved:雪😀', inherited = null) {
  const trace = [];
  let result;
  try {
    const value = raw === undefined ? undefined : JSON.parse(raw);
    const resolver = id => { trace.push(id); return resolved === null ? undefined : resolved; };
    result = op === 'number' ? api.toNumber(value) : op === 'optional' ? api.toOptionalNumber(value) : api.mapBlofinHistoricalOrder(value,resolver);
    result = {ok: observe(result)};
  } catch (error) { result = {error: error.name, message: error.message}; }
  cases.push({op, raw: raw ?? null, resolved, trace, result, inherited});
}
for (const raw of [undefined,...inputs]) { run('number',raw); run('optional',raw); }
const base = {orderId:'order:unique', instId:'BTC-USDT', createTime:'1712345600123', side:'buy', positionSide:'short', orderType:'market', state:'canceled', price:'17.25', averagePrice:'18.75', size:'3.5', filledSize:'2.125', reduceOnly:'true', updateTime:'1712345699876'};
for (const resolved of [null,'','mapped:😀']) run('order',JSON.stringify(base),resolved);
for (const key of Object.keys(base)) {
  const absent = {...base}; delete absent[key]; run('order',JSON.stringify(absent));
  for (const raw of inputs) {
    const parts = Object.entries(base).map(([k,v]) => JSON.stringify(k)+':'+(key === k ? raw : JSON.stringify(v)));
    run('order','{'+parts.join(',')+'}');
  }
}
for (const [key, table] of [['orderType',api.ORDER_TYPE],['state',api.ORDER_STATUS]]) {
  for (const value of [...Object.keys(table), ...Object.keys(table).map(v => v.toUpperCase()), 'missing', ...Object.getOwnPropertyNames(Object.getPrototypeOf(table))]) {
    const inherited = ['constructor','__proto__'].includes(value) ? {field:key === 'orderType' ? 'type' : 'status',name:value} : null;
    run('order',JSON.stringify({...base,[key]:value}),'mapped:😀',inherited);
  }
}
for (const raw of ['null','[]','{}','true','"text"']) run('order',raw);
run('order', '{"orderId":"x","instId":"y","createTime":"1","createTime":-0,"side":"buy"}');
const boundaries = ['"\\ud800"', '{"orderId":"x","instId":"\\ud800","createTime":"1","side":"buy"}', '['.repeat(140)+'0'+']'.repeat(140)];
assert.equal(api.toNumber(JSON.parse(boundaries[0])),0,'source_history_surrogate_numeric');
assert.equal(api.mapBlofinHistoricalOrder(JSON.parse(boundaries[1]),()=>undefined).symbol.charCodeAt(0),0xd800,'source_history_surrogate_symbol');
assert.equal(api.toNumber(JSON.parse(boundaries[2])),0,'source_history_deep_numeric');
process.stdout.write(JSON.stringify({ header: {architecture: process.arch, nodeMajor: Number(process.versions.node.split('.')[0]), nodeVersion: process.version}, cases,boundaries}));
