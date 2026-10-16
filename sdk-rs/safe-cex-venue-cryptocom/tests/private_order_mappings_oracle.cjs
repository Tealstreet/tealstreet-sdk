const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'PRIVATE-MAPPING compiler path required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../../packages/safe-cex/src');
function execute(relative, requireModule) {
  const file = path.join(root, relative);
  const compiled = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
    fileName: file, reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  });
  assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  const context = vm.createContext({ exports: {}, require: requireModule });
  vm.runInContext(compiled.outputText, context, { filename: file });
  return context.exports;
}
const logging = execute('../../logging/src/LogSeverity.ts', name => { throw Error('unexpected logging import ' + name); });
const models = execute('types.ts', name => {
  if (name === '@tealstreet/logging/src/LogSeverity') return logging;
  throw Error('unexpected model import ' + name);
});
const file = path.join(root, 'exchanges/cryptocom/cryptocom.ws-private.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const classes = source.statements.filter(n => ts.isClassDeclaration(n) && n.name?.text === 'CryptocomPrivateWebSocket');
assert.equal(classes.length, 1, 'PRIVATE-MAPPING actual class');
let touches = 0, coercions = 0;
const receiver = new Proxy({}, { get() { touches++; throw Error('stateless mapping read receiver'); }, set() { touches++; throw Error('stateless mapping wrote receiver'); } });
const throwing = () => { coercions++; throw Error('switch coerced argument'); };
const nonStrings = [
  ['undefined', undefined], ['null', null], ['false', false], ['true', true], ['zero', 0],
  ['negative', -1], ['nan', NaN], ['infinity', Infinity], ['object', {}], ['array', []],
  ['boxed', new String('MARKET')], ['symbol', Symbol('MARKET')], ['bigint', 1n], ['function', () => {}],
  ['coercion-trap', { [Symbol.toPrimitive]: throwing, valueOf: throwing, toString: throwing }],
  ['proxy-trap', new Proxy({}, { get: throwing })],
];
assert.equal(nonStrings.length, 16, 'PRIVATE-MAPPING independent non-string inventory');
const groups = [
  ['mapOrderType', ['MARKET', 'LIMIT', 'STOP_LOSS', 'STOP_LIMIT', 'TAKE_PROFIT', 'TAKE_PROFIT_LIMIT', '', 'UNKNOWN', 'market']],
  ['mapOrderStatus', ['ACTIVE', 'PENDING', 'FILLED', 'CANCELED', 'EXPIRED', 'REJECTED', '', 'UNKNOWN', 'active']],
  ['mapTimeInForce', ['GOOD_TILL_CANCEL', 'IMMEDIATE_OR_CANCEL', 'FILL_OR_KILL', 'POST_ONLY', '', 'UNKNOWN', 'immediate_or_cancel']],
];
function units(value) { return Array.from({ length: value.length }, (_, i) => value.charCodeAt(i)); }
const captures = groups.map(([method, strings]) => {
  const fields = classes[0].members.filter(n => n.name?.getText(source) === method);
  assert.equal(fields.length, 1, 'PRIVATE-MAPPING actual field ' + method);
  assert(ts.isArrowFunction(fields[0].initializer));
  const compiled = ts.transpileModule(`return (${fields[0].initializer.getText(source)});`, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
  const mapping = new Function('OrderType', 'OrderStatus', 'OrderTimeInForce', '"use strict";\n' + compiled).call(receiver, models.OrderType, models.OrderStatus, models.OrderTimeInForce);
  const exotic = ['\ud800', '\udfff', '🙂', strings[0] + '\ud800', '\udfff' + strings[0]];
  const rows = [...strings, ...exotic].map(input => ({ label: 'string', units: units(input), result: mapping(input) }));
  for (const [label, input] of nonStrings) rows.push({ label, units: null, result: mapping(input) });
  assert(rows.every(r => typeof r.result === 'string'), 'PRIVATE-MAPPING native finite enum values');
  return { method, rows };
});
assert.equal(touches, 0, 'PRIVATE-MAPPING source receiver effects');
assert.equal(coercions, 0, 'PRIVATE-MAPPING source coercion effects');
assert.deepEqual(captures.map(c => c.rows.length), [30, 30, 28], 'PRIVATE-MAPPING independent group counts');
const inventory = Object.fromEntries(['OrderType', 'OrderStatus', 'OrderTimeInForce'].map(name => [name, Object.entries(models[name])]));
process.stdout.write(JSON.stringify({ captures, inventory, touches, coercions }));
