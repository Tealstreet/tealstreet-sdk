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
  if (file.endsWith('blofin.income.ts')) source += '\nexport { toNumber };';
  const output = ts.transpileModule(source, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
  vm.runInNewContext(output, {exports, require(spec) {
    if (spec.startsWith('@tealstreet/')) return execute(path.join(root, 'packages', spec.slice('@tealstreet/'.length) + '.ts'));
    assert(spec.startsWith('.'), `unexpected_dependency:${spec}`);
    const target = path.resolve(path.dirname(file), spec);
    return execute(fs.existsSync(target + '.ts') ? target + '.ts' : path.join(target, 'index.ts'));
  }}, {filename: file});
  return exports;
}
const api = execute(path.join(root, 'packages/safe-cex/src/exchanges/blofin/blofin.income.ts'));
function observe(value) {
  if (typeof value === 'number') {
    const b = Buffer.alloc(8); b.writeDoubleBE(Number.isNaN(value) ? NaN : value);
    return {number: Number.isNaN(value) ? 'nan' : b.toString('hex')};
  }
  if (Array.isArray(value)) return Array.from(value, observe);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v]) => [k,observe(v)]));
  return value;
}
const inputs = ['null','true','false','0','-0','1e400','-1e400','-1e-400','9007199254740993','1.25','[]','[null]','["-2.5tail"]','{}','{"toString":null}'];
for (const s of ['', ' ', '\ufeff-0junk', '\u008512', '1.25e2tail', '-3.75', '+Infinity', 'NaN', '0x10', '1e', '.5tail', '-5e-324', '1e309', '12😀', '雪', 'SHORT', 'TRUE']) inputs.push(JSON.stringify(s));
const cases = [];
function run(op, raw, resolved = 'resolved:雪😀') {
  const trace = [];
  let result;
  try {
    const value = raw === undefined ? undefined : JSON.parse(raw);
    const resolver = id => { trace.push(id); return resolved === null ? undefined : resolved; };
    result = op === 'number' ? api.toNumber(value) : op === 'fill' ? api.mapBlofinFillToIncomeRecords(value, resolver) : api.mapBlofinClosedPosition(value, resolver, 'account:unique');
    result = {ok: observe(result)};
  } catch (error) { result = {error: error.name, message: error.message}; }
  cases.push({op, raw: raw ?? null, resolved, trace, result});
}
for (const raw of [undefined, ...inputs]) run('number', raw);
const fill = {ts:'1712345678901', instId:'BTC-USDT', tradeId:'trade:unique', fillPnl:'12.75', fee:'0.125'};
const closed = {createTime:'1712345600123', updateTime:'1712345699876', instId:'ETH-USDT', closeAveragePrice:'17.5', closePositions:'3.25', openAveragePrice:'11.75', realizedPnl:'-2.875', positionSide:'short', fee:'-0.375', leverage:'9.5'};
for (const [op, base] of [['fill', fill], ['closed',closed]]) {
  for (const resolved of [null, '', 'resolved:雪😀']) run(op, JSON.stringify(base), resolved);
  for (const key of Object.keys(base)) {
    const absent = {...base}; delete absent[key]; run(op, JSON.stringify(absent));
    for (const raw of inputs) {
      const parts = Object.entries(base).map(([k,v]) => JSON.stringify(k)+':'+(k === key ? raw : JSON.stringify(v)));
      run(op, '{'+parts.join(',')+'}');
    }
  }
  for (const raw of ['null','[]','true','"string"','{}']) run(op,raw);
}
run('fill', '{"ts":"12","ts":-0,"instId":"X","fillPnl":2}');
run('fill', '{"ts":"'+ '9'.repeat(400)+'","instId":"X","fillPnl":1}');
run('closed', JSON.stringify({...closed, closePositions:'1e308', closeAveragePrice:'1e308'}));
run('closed', JSON.stringify({...closed, closePositions:'-0', closeAveragePrice:'-2'}));
const boundaries = ['"\\ud800"', '{"ts":"12","instId":"\\ud800","fillPnl":2}', '['.repeat(140)+'0'+']'.repeat(140)];
// Source succeeds on these outside the declared shared UTF-8/depth ingress domain.
for (const raw of boundaries) assert.doesNotThrow(() => JSON.parse(raw), 'source_boundary_acceptance');
assert.equal(api.toNumber(JSON.parse(boundaries[0])), 0, 'source_surrogate_numeric');
assert.equal(api.mapBlofinFillToIncomeRecords(JSON.parse(boundaries[1]), () => undefined)[0].symbol.charCodeAt(0), 0xd800, 'source_surrogate_symbol_unit');
assert.equal(api.toNumber(JSON.parse(boundaries[2])), 0, 'source_deep_numeric');
process.stdout.write(JSON.stringify({ header: {architecture: process.arch, nodeMajor: Number(process.versions.node.split('.')[0]), nodeVersion: process.version}, cases,boundaries}));
