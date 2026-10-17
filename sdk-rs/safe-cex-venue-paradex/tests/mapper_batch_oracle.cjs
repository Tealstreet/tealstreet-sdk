const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
process.env.TZ = 'UTC';
const read = p => fs.readFileSync(path.join(root, p), 'utf8');
function execute(code, globals = {}) {
  const module = { exports: {} };
  const result = ts.transpileModule(code, { reportDiagnostics: true, compilerOptions: { module: 1, target: 9 } });
  assert.equal(result.diagnostics.length, 0, 'batch_transpile');
  vm.runInNewContext(result.outputText, { module, exports: module.exports, ...globals });
  return module.exports;
}
function parse(file) {
  const source = ts.createSourceFile(file, read(file), 99, true);
  assert.equal(source.parseDiagnostics.length, 0, 'batch_source_parse');
  return source;
}
const source = parse('packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts');
const cls = source.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'ParadexExchange');
const names = ['mapOrder', 'mapTimeInForce', 'mapOrderType', 'reverseMapOrderType', 'mapOrderStatus'];
const members = names.map(name => {
  const nodes = cls.members.filter(n => n.name?.getText(source) === name);
  assert.equal(nodes.length, 1, 'batch_source_identity');
  return nodes[0].getText(source);
});
const shared = parse('packages/safe-cex/src/types.ts');
const enums = execute(shared.statements.filter(n => ts.isEnumDeclaration(n) &&
  ['OrderType', 'OrderSide', 'OrderStatus'].includes(n.name.text)).map(n => n.getText(shared)).join('\n'));
const constants = execute(read('packages/safe-cex/src/exchanges/paradex/paradex.types.ts'), { require: () => enums });
const { Harness } = execute(`export class Harness { ${members.join('\n')} }`, { ...enums, ...constants });
const h = new Harness();
const ws = parse('packages/safe-cex/src/exchanges/paradex/paradex.ws-public.ts');
const declarations = ws.statements.filter(ts.isVariableStatement).flatMap(n => [...n.declarationList.declarations])
  .filter(n => n.name.getText(ws) === 'getOrderBookChannel');
assert.equal(declarations.length, 1, 'batch_channel_identity');
const { getOrderBookChannel } = execute(`export const ${declarations[0].getText(ws)};`);
const prototypes = ['constructor', '__defineGetter__', '__defineSetter__', 'hasOwnProperty', '__lookupGetter__',
  '__lookupSetter__', 'isPrototypeOf', 'propertyIsEnumerable', 'toString', 'valueOf', '__proto__', 'toLocaleString'];
const tifKeys = [null, '', 'GoodTillCancel', 'GTC', 'ImmediateOrCancel', 'IOC', 'PostOnly', 'POST_ONLY',
  'GTT', 'FOK', ...prototypes, 'unknown', 'é😀', 'fok'];
const kinds = ['market', 'limit', 'conditional', 'stop_market', 'take_profit_market', 'trailing_stop_market'];
const channels = ['BTC-USD-PERP', '', 'é😀', 'constructor', 'a.b', '\0', 'x\n'];
function observe(value) {
  if (typeof value === 'number') {
    const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
    return { kind: 'number', bits: Number.isNaN(value) ? '7ff8000000000000' : bytes.toString('hex') };
  }
  if (typeof value === 'function') return { kind: 'function', text: String(value) };
  if (value === null) return { kind: 'null' };
  if (Array.isArray(value)) return { kind: 'array', entries: value.map(observe) };
  if (typeof value === 'object') return { kind: 'object', entries: Object.entries(value).map(([k, v]) => [k, observe(v)]) };
  return { kind: typeof value, value };
}
const base = { id: 'order-A', market: 'BTC-USD-PERP', side: 'BUY', type: 'LIMIT', status: 'NEW',
  price: '123.75', trigger_price: '246.5', size: '17.25', remaining_size: '3.5',
  created_at: '2026-10-12T08:09:10.123Z', updated_at: '2026-10-13T11:12:13Z',
  flags: ['REDUCE_ONLY'], reduce_only: false, client_id: 'client-original', time_in_force: 'IOC', extra: { marker: 'raw-extra' } };
const defs = [];
function add(patch = {}, remove = [], refused = false) {
  const row = { ...base, ...patch }; for (const key of remove) delete row[key];
  defs.push({ raw: JSON.stringify(row), refused });
}
for (const type of ['LIMIT', 'MARKET', 'STOP_LIMIT', 'STOP_MARKET', 'STOP_LOSS_LIMIT', 'STOP_LOSS_MARKET', 'TAKE_PROFIT_LIMIT', 'TAKE_PROFIT_MARKET'])
  for (const reduce_only of [false, true]) for (const side of ['BUY', 'SELL']) add({ type, reduce_only, side });
add({ type: 'unknown', status: 'unknown' });
for (const status of ['NEW', 'OPEN', 'PENDING', 'UNTRIGGERED', 'FILLED', 'CANCELED', 'REJECTED']) add({ status });
for (const patch of [{ price: '' }, { price: '-0' }, { price: 'bad' }, { price: 'Infinity' }, { price: ' 19.5tail' },
  { type: 'STOP_MARKET', trigger_price: '' }, { type: 'STOP_LIMIT', trigger_price: '-0' },
  { type: 'TAKE_PROFIT_MARKET', trigger_price: '73.125' }]) add(patch);
const numbers = ['0', '-0', '-1e-9999', '1e400', '-1e400', '9007199254740993', '1.0000000000000002', '17.25x'];
for (const size of numbers) add({ size });
for (const created_at of ['not-a-date', '', '2026-10-12T08:09:10.123', '2026-10-12T08:09:10.123+03:30', '+275760-09-13T00:00:00.001Z']) add({ created_at });
add({ reduce_only: null }); add({ reduce_only: 'yes' });
for (const flags of [['REDUCE_ONLY'], null, ['reduce_only'], ['x', 'REDUCE_ONLY'], 'REDUCE_ONLY']) add({ flags }, ['reduce_only']);
const feeder = read('sdk-rs/safe-cex-venue-paradex/tests/utilities_oracle.cjs');
const numericRaw = vm.runInNewContext(feeder.slice(feeder.indexOf('const scalarRaw='), feeder.indexOf('const scalars=')) + 'scalarRaw')
  .filter(raw => typeof JSON.parse(raw) === 'number');
assert.equal(numericRaw.length, 8, 'batch_numeric_feeder_count');
for (const raw of numericRaw) defs.push({ raw: JSON.stringify({ ...base, extra: { token: 'TOKEN' } }).replace('"TOKEN"', raw), refused: false });
for (const field of ['side', 'type', 'status']) for (const key of prototypes) add({ [field]: key }, [], true);
for (const patch of [{ side: 'unknown' }, { side: '' }, { id: null }]) add(patch, [], true);
add({ created_at: 7 });
for (const raw of ['1784505600000', '-1784505600000', '7.9', '-7.9', '0', '-0',
  '8640000000000000', '8640000000000001', '-8640000000000000', '-8640000000000001', '1e400', '-1e400']) {
  defs.push({ raw: JSON.stringify({ ...base, created_at: 'TOKEN' }).replace('"TOKEN"', raw), refused: false });
}
for (const created_at of [null, false, true]) add({created_at});
add({}, ['created_at']);
for (const field of ['size', 'remaining_size']) {
  for (const raw of ['0.006', '-0', 'null', 'true', 'false', '["17.25tail"]', '{}']) {
    defs.push({raw: JSON.stringify({...base, [field]: 'TOKEN'}).replace('"TOKEN"', raw), refused: false});
  }
  add({}, [field]);
}
for (const created_at of [{}, [], [null], ['2026-07-20T00:00:00Z']]) add({created_at});
assert.equal(defs.length, 152, 'batch_fixture_count');
const orders = defs.map(c => {
  const input = JSON.parse(c.raw), output = h.mapOrder(input);
  return { ...c, input: observe(input), output: observe(output), ownsUndefined: Object.hasOwn(output, 'timeInForce'), aliasesInput: output.info === input };
});
const aliasInput = JSON.parse(JSON.stringify(base)), aliasOutput = h.mapOrder(aliasInput);
aliasInput.extra.marker = 'changed-after-map';
const lone = String.fromCharCode(0xd800);
const units = value => Array.from({ length: value.length }, (_, i) => value.charCodeAt(i));
process.stdout.write(JSON.stringify({ header: { arch: process.arch, node: process.version, nodeMajor: Number(process.versions.node.split('.')[0]), timezone: 'UTC' },
  orders, timeInForce: tifKeys.map(key => ({ key, output: observe(h.mapTimeInForce(key === null ? undefined : key)) })),
  types: kinds.map(key => ({ key, output: h.mapOrderType(key) })),
  channels: channels.map(key => ({ key, output: getOrderBookChannel(key) })),
  boundary: { aliasMarker: aliasOutput.info.extra.marker, surrogateRaw: JSON.stringify(lone),
    surrogateTif: h.mapTimeInForce(lone), surrogateChannel: units(getOrderBookChannel(lone)),
    surrogateOrder: units(h.mapOrder({ ...base, id: lone }).id) }
}));
