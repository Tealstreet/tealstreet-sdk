const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const sourcePath = path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.order-history.ts');
let source = fs.readFileSync(sourcePath, 'utf8');
if (process.argv.includes('--control')) {
  const altered = source.replace("if (status === 'filled') return 2;", "if (status === 'filled') return 0;");
  assert.notEqual(altered, source, 'status-rank production mutation must apply');
  source = altered;
}
if (process.argv.includes('--alias-control')) {
  const altered = source.replace('latest.set(oid, row);', 'latest.set(oid, {...row});');
  assert.notEqual(altered, source, 'live-alias production mutation must apply');
  source = altered;
}
const typesPath = path.join(root, 'packages/safe-cex/src/types.ts');
const types = ts.createSourceFile(typesPath, fs.readFileSync(typesPath, 'utf8'), ts.ScriptTarget.Latest, true);
const wanted = ['OrderSide', 'OrderStatus', 'OrderType'];
const selected = types.statements.filter(node => ts.isEnumDeclaration(node) && wanted.includes(node.name.text));
assert.equal(selected.length, wanted.length, 'actual shared enum declarations required');
const enumExports = {};
vm.runInNewContext(ts.transpileModule(selected.map(node => ts.createPrinter().printNode(ts.EmitHint.Unspecified, node, types)).join('\n'), {compilerOptions: {module: ts.ModuleKind.CommonJS}}).outputText, {exports: enumExports});
const exportsOut = {};
vm.runInNewContext(ts.transpileModule(source, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText + '\nexports.__helpers = {toNumber,toOptionalNumber,statusRank,mapStatus,isLaterState};', {exports: exportsOut, require: name => { assert.equal(name, '../../types'); return enumExports; }});
const cases = [];
function capture(operation, raw, resolverThrows = false) {
  const row = {operation, raw};
  if (raw.includes('\\ud800') || raw.includes('\\udfff')) row.representationRefusal = true;
  if (resolverThrows) row.resolverThrows = true;
  try {
    const value = exportsOut.__helpers[operation] ? exportsOut.__helpers[operation](...JSON.parse(raw)) : operation === 'collapse' ? exportsOut.collapseHyperliquidOrderHistory(JSON.parse(raw)) : exportsOut.mapHyperliquidHistoricalOrder(JSON.parse(raw), coin => { if (resolverThrows) throw new TypeError('resolver refused coin'); return coin === 'BTC' ? 'BTC-PERP' : undefined; });
    row.value = value;
    // Capture before JSON.stringify can erase inherited functions or Infinity.
    if (operation === 'map' && value) {
      row.sideType = typeof value.side; row.orderTypeType = typeof value.type;
      row.sideString = String(value.side); row.orderTypeString = String(value.type);
      row.amountString = String(value.amount); row.filledString = String(value.filled);
    }
  } catch (error) { row.errorName = error.name; row.error = error.message; }
  cases.push(row);
}
const base = {order: {oid: 1, coin: 'BTC', timestamp: 1, side: 'B', origSz: '10', sz: '2', limitPx: '100', orderType: 'limit'}, status: 'open', statusTimestamp: 2};
if(process.argv.includes('--field-identities')) {
 for(const side of ['A','B'])for(const kind of ['limit','market','stop market','take profit limit']) {
  capture('map',JSON.stringify({order:{oid:137,coin:'BTC',timestamp:1234,side,origSz:'17',sz:'5',limitPx:'91',orderType:kind,reduceOnly:side==='A'},status:'filled',statusTimestamp:5678}));
 }
 const target=path.join(__dirname,'../tests/fixtures/history-field-identities.json');const output=JSON.stringify({label:'Self-authored native field-identity supplement; original history artifacts unchanged',cases},null,2)+'\n';
 if(process.argv.includes('--capture'))fs.writeFileSync(target,output,{flag:'wx'});else assert.equal(output,fs.readFileSync(target,'utf8'));console.log(cases.length+' history field identity cases');return;
}
for (const key of Object.getOwnPropertyNames(Object.prototype)) {
  capture('map', JSON.stringify({...base, order: {...base.order, side: key}}));
  capture('map', JSON.stringify({...base, order: {...base.order, orderType: key}}));
}
for (const patch of [{side: 'A'}, {side: 'missing'}, {side: 'CONSTRUCTOR'}, {orderType: 'STOP MARKET'}, {orderType: 'take profit limit'}, {coin: 'delisted'}, {origSz: '12abc', sz: '0x10'}, {timestamp: 0}, {oid: 0}, {reduceOnly: true}, {reduceOnly: 'true'}, {limitPx: 0}, {timestamp: '\ufeff12tail'}, {timestamp: '\u008512'}, {oid: {toString: null}}]) capture('map', JSON.stringify({...base, order: {...base.order, ...patch}}));
for (const raw of ['null', '{}', '[]', '123', '{"order":{"oid":"x","coin":"BTC","timestamp":1,"side":"B","origSz":"1e308","sz":"-1e308"},"status":"filled"}', '{"order":{"oid":"x","coin":"BTC","timestamp":1,"side":"B","origSz":{"toString":null}}}']) capture('map', raw);
const order = (id, status, time) => ({order: {oid: id}, status, statusTimestamp: time});
for (const rows of [[], [null, {}, order('', 'filled', 1)], [order('1', 'open', 10), order('1', 'filled', 10)], [order('1', 'filled', 10), order('1', 'open', 10)], [order('2', 'open', 5), order('1', 'filled', 5), order('2', 'canceled', 6)], [order('1', 'canceled', 10), order('1', 'filled', 10)], [order('1', 'filled', 10), order('1', 'canceled', 10)], [order('1', 'filled', '10junk'), order('1', 'open', '11junk')], [order('1', 'newTerminalStatus', 4), order('1', 'triggered', 4)]]) capture('collapse', JSON.stringify(rows));
if (process.argv.includes('--boundaries')) {
  cases.length = 0;
  for (const raw of [
    '{"order":{"oid":"\\ud800","coin":"BTC","timestamp":1,"side":"B"}}',
    '{"order":{"oid":"x","coin":"\\udfff","timestamp":1,"side":"B"}}',
    '{"order":{"oid":"😀","coin":"BTC","timestamp":1,"side":"B"}}',
    '{"order":{"oid":"x","coin":"BTC","timestamp":1,"side":"constructor"},"status":{"toString":null}}'
  ]) capture('map', raw);
  capture('collapse', '[{"order":{"oid":"雪😀\\ufeff"}}]');
  for (const raw of ['[{"order":{"oid":"\\ud800"}}]', '[{"order":{"oid":"\\udfff"}}]']) capture('collapse', raw);
  for (const row of cases) {
    if (row.raw.includes('\\ud800') || row.raw.includes('\\udfff')) {
      row.sourceJson = JSON.stringify(row.value);
      row.representationRefusal = true;
      delete row.value;
    }
  }
  const raw = JSON.stringify([order('1', 'open', 1)]);
  const input = JSON.parse(raw);
  const output = exportsOut.collapseHyperliquidOrderHistory(input);
  const before = JSON.parse(JSON.stringify(output));
  input[0].status = 'filled';
  cases.push({operation: 'live-alias', raw, before, after: output, sameReference: output[0] === input[0]});
}
if (process.argv.includes('--helpers')) {
  cases.length = 0;
  for (const operation of ['toNumber', 'toOptionalNumber']) {
    for (const raw of ['[null]', '[0]', '[-0]', '[1e400]', '["0x10"]', '["12abc"]', '["\\ufeff12tail"]', '["\\u008512"]', '[{"toString":null}]']) capture(operation, raw);
  }
  for (const operation of ['toNumber', 'toOptionalNumber']) {
    for (const raw of ['["\\ud800"]', '["\\udfff"]']) capture(operation, raw);
  }
  for (const raw of ['[{"status":"\\ud800"},{}]', '[{"status":"\\udfff"},{}]']) capture('isLaterState', raw);
  for (const operation of ['mapStatus', 'statusRank']) {
    for (const raw of ['["open"]','["triggered"]','["filled"]','["unknown-terminal"]']) capture(operation, raw);
  }
  capture('map', JSON.stringify(base), true);
  for (const raw of [
    '[{"statusTimestamp":{"toString":null}},null]',
    '[null,{"statusTimestamp":{"toString":null}}]',
    '[{},null]',
    '[{"statusTimestamp":2},{"statusTimestamp":1}]',
    '[{"statusTimestamp":1,"status":"filled"},{"statusTimestamp":1,"status":"open"}]'
  ]) capture('isLaterState', raw);
}
const fixture = path.join(__dirname, process.argv.includes('--helpers') ? '../tests/fixtures/order-history-helpers.json' : process.argv.includes('--boundaries') ? '../tests/fixtures/order-history-boundaries.json' : '../tests/fixtures/order-history.json');
const text = JSON.stringify(cases, null, 2) + '\n';
if (process.argv.includes('--capture')) fs.writeFileSync(fixture, text, {flag: 'wx'});
else assert.equal(text, fs.readFileSync(fixture, 'utf8'), 'actual order-history oracle drift');
