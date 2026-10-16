const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/polymarket');
const api = fs.readFileSync(path.join(root, 'polymarket.api.ts'), 'utf8');
const tree = ts.createSourceFile('api.ts', api, ts.ScriptTarget.Latest, true);
const cls = tree.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'PolymarketAPI');
const names = ['getMarkets', 'getOrderBook', 'getOrderBooks', 'getPrices'];
const methods = names.map(name => {
 const selected = cls.members.filter(n => n.name?.getText(tree) === name);
 assert.equal(selected.length, 1, `actual_method:${name}`);
 return api.slice(selected[0].getFullStart(), selected[0].end);
});
const constants = fs.readFileSync(path.join(root, 'polymarket.constants.ts'), 'utf8');
const ct = ts.createSourceFile('constants.ts', constants, ts.ScriptTarget.Latest, true);
const decl = ct.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(ct) === 'ENDPOINTS'));
assert.equal(decl.length, 1, 'actual_endpoint_declaration');
const source = constants.slice(decl[0].getFullStart(), decl[0].end) + '\nclass PolymarketAPI {\n' + methods.join('\n') + '\n}\nexports.API=PolymarketAPI;';
const context = { exports: {} };
vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText, context);
const market = (id) => ({ condition_id: `condition-${id}`, token_id: `token-${id}`, outcome: `outcome-${id}`, minimum_order_size: '3.500', minimum_tick_size: '0.0010', active: id === 1, closed: id !== 1 });
const book = (id) => ({ asset_id: `asset-${id}`, bids: [{ price: '0.10', size: '13.000' }, { price: '0.09', size: '17' }], asks: [{ price: '0.80', size: '23.50' }], timestamp: 17.25, hash: `hash-${id}` });
const price = (id) => ({ token_id: `price-token-${id}`, price: id === 1 ? '0.4700' : '0.53', timestamp: 19.5 });
function fixture(name) {
 if (name === 'getMarkets') return [market(1), market(2)];
 if (name === 'getOrderBook') return book(1);
 if (name === 'getOrderBooks') return [book(1), book(2)];
 return [price(1), price(2)];
}
function first(value) { return Array.isArray(value) ? value[0] : value; }
function bits(value) { const buffer = Buffer.alloc(8); buffer.writeDoubleBE(value); return buffer.toString('hex'); }
(async () => {
 const cases = [], failures = [];
 for (const name of names) {
  for (let iteration = 0; iteration < 9; iteration++) {
   let data = fixture(name), accepted = true;
   const item = first(data);
   if (iteration === 1) { data = name === 'getOrderBook' ? { asset_id: '', bids: [], asks: [] } : []; }
   if (iteration === 2) { item[name === 'getOrderBook' || name === 'getOrderBooks' ? 'asset_id' : 'token_id'] = '🦀 λ e\u0301'; }
   if (iteration === 3) { item.extra = { retainedByTS: true }; }
   if (iteration === 4) { delete item[name === 'getOrderBook' || name === 'getOrderBooks' ? 'asset_id' : 'token_id']; accepted = false; }
   if (iteration === 5) { data = null; accepted = false; }
   if (iteration === 6) { data = JSON.stringify(data); accepted = false; }
   let raw = JSON.stringify(data);
   if (iteration === 7) {
    if (name === 'getMarkets') { item.minimum_tick_size = '\ud800'; accepted = false; raw = JSON.stringify(data); }
    else { raw = raw.replace('"timestamp":17.25', '"timestamp":-0').replace('"timestamp":19.5', '"timestamp":1e400'); }
   }
   if (iteration === 8) {
    if (name === 'getMarkets') { item.minimum_order_size = ''; raw = JSON.stringify(data); }
    else { raw = raw.replace('"timestamp":17.25', '"timestamp":-1e-400').replace('"timestamp":19.5', '"timestamp":9007199254740993'); }
   }
   const input = iteration === 1 ? [] : iteration === 2 ? ['🦀','a,b',''] : ['token-1','token-2','token-1'];
   data = JSON.parse(raw);
   const calls = [], instance = new context.exports.API();
   instance.xhr = {};
   for (const verb of ['get','post']) instance.xhr[verb] = async (...args) => { calls.push({ verb, args }); return { data }; };
   const args = name === 'getMarkets' ? [] : name === 'getOrderBook' ? [input[0] ?? ''] : [input];
   const output = await instance[name](...args);
   assert.strictEqual(output, data, `source_identity:${name}:${iteration}`);
   const rows = output === null || typeof output === 'string' ? [] : Array.isArray(output) ? output : [output];
   const timestamps = rows.map(row => row.timestamp === undefined ? null : bits(row.timestamp));
   cases.push({ name, iteration, raw, input, accepted, encodedOnly: name === 'getMarkets' && iteration === 7, output: name === 'getMarkets' && iteration === 7 ? null : output, timestamps, calls });
  }
  const instance = new context.exports.API(), error = { code: 'ERR_NETWORK', message: 'fixture failure' }, calls = [];
  instance.xhr = {};
  for (const verb of ['get','post']) instance.xhr[verb] = async (...args) => { calls.push({ verb, args }); throw error; };
  try { await instance[name](...(name === 'getMarkets' ? [] : name === 'getOrderBook' ? ['x'] : [['x','y']])); assert.fail('source rejection required'); }
  catch (actual) { assert.strictEqual(actual, error, `source_failure_identity:${name}`); failures.push({ name, preserved: true, calls }); }
 }
 process.stdout.write(JSON.stringify({ capture: { architecture: process.arch, nodeMajor: Number(process.versions.node.split('.')[0]) }, cases, failures }));
})().catch(error => { console.error(error); process.exitCode = 1; });
