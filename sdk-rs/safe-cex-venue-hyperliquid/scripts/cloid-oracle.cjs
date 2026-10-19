const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const read = (relative) => fs.readFileSync(path.join(root, 'packages/safe-cex/src', relative), 'utf8');
const compilerOptions = { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 };
const typesFile = path.join(root, 'packages/safe-cex/src/types.ts');
const types = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'), ts.ScriptTarget.Latest, true);
const enums = types.statements.filter(
  (node) => ts.isEnumDeclaration(node) && ['OrderType', 'OrderSide', 'OrderTimeInForce'].includes(node.name.text),
);
assert.equal(enums.length, 3, 'actual shared cloid enums');
const shared = {};
vm.runInNewContext(ts.transpileModule(enums.map((node) => node.getText(types)).join('\n'), { compilerOptions }).outputText, {
  exports: shared,
});
const load = (relative, requires, context = {}) => {
  const exports = {};
  vm.runInNewContext(ts.transpileModule(read(relative), { compilerOptions }).outputText, {
    exports,
    require: (name) => {
      assert.ok(name in requires, `unexpected import ${name} from ${relative}`);
      return requires[name];
    },
    ...context,
  });
  return exports;
};
const inference = load('exchanges/hyperliquid/hyperliquid.order-inference.ts', { '../../types': shared });
const clientOrderId = load('exchanges/clientOrderId.ts', { '../types': shared, '../utils/uuid': { uuid: () => 'unused' } });
// One controllable CSPRNG: each mint fills the next fixed 16 bytes.
let draws = [];
const cloid = load(
  'exchanges/hyperliquid/hyperliquid.cloid.ts',
  { '../clientOrderId': clientOrderId, '../../types': shared, './hyperliquid.order-inference': inference },
  { crypto: { getRandomValues: (bytes) => (bytes.set(draws.shift()), bytes) }, Math },
);
const bits = (value) => {
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
};
const cases = [];
const run = (row, body) => {
  try {
    Object.assign(row, body());
  } catch (error) {
    row.errorName = error.name;
    row.error = error.message;
  }
  cases.push(row);
};
const hex32 = '0123456789abcdef0123456789abcdef';
for (const value of [
  `0x${hex32}`, `0x${hex32.toUpperCase()}`, `0X${hex32}`, `0x${hex32.slice(1)}`, `0x${hex32}0`, `${hex32}00`,
  `0x${hex32.slice(1)}g`, `0x${hex32}\n`, ` 0x${hex32}`, '', '0x',
]) {
  run({ operation: 'pattern', value }, () => ({ result: cloid.HYPERLIQUID_CLOID_PATTERN.test(value) }));
}
for (const raw of [`"0x${hex32}"`, `"0x${hex32.toUpperCase()}"`, 'null', '7', 'true', `["0x${hex32}"]`, '{}', '""']) {
  run({ operation: 'read', raw }, () => ({ result: cloid.readHyperliquidCloid(JSON.parse(raw)) ?? null }));
}
const rule = cloid.hyperliquidClientOrderIdRule;
run({ operation: 'rule' }, () => ({
  label: rule.label,
  requirement: rule.requirement,
  valid: [`0x${hex32}`, `0x${hex32.toUpperCase()}`].map((id) => rule.isValid(id)),
  honorable: Object.fromEntries(Object.values(shared.OrderType).map((type) => [type, rule.isHonorableShape({ type })])),
  mints: typeof rule.mint,
}));
for (const bytes of [Array(16).fill(0), Array(16).fill(255), [...Array(16).keys()], [16, 1, 2, 171, 205, 239, 0, 9, 10, 15, 128, 127, 64, 32, 8, 4]]) {
  draws = [Uint8Array.from(bytes)];
  run({ operation: 'mint', bytes }, () => ({ result: cloid.mintHyperliquidCloid() }));
}
const limit = (tif) => ({ limit: { tif } });
const trigger = (tpsl, triggerPx) => ({ trigger: { triggerPx, isMarket: true, tpsl } });
const wires = [
  { a: 0, b: true, p: '91.25', s: '0.5', r: false, t: limit('Gtc'), c: `0x${hex32}` },
  { a: 1, b: false, p: '1e3', s: '2', r: true, t: limit('Alo') },
  { a: 2, b: true, p: '-0', s: '0.001', r: false, t: limit('Ioc') },
  { a: 3, b: false, p: '12', s: '3', r: true, t: trigger('tp', '17.5') },
  { a: 4, b: true, p: '12', s: '3', r: true, t: trigger('sl', '23.75') },
  { a: 5, b: true, p: 'abc', s: ' 4 ', r: false, t: limit('Gtc') },
  { a: 6, b: true, p: '5', s: '1', r: false, t: {} },
  { a: 7, b: false, p: '5', s: '1', r: false, t: { trigger: null, limit: { tif: 'Gtc' } } },
  { a: 8, b: true, p: '5', s: '1', r: false, t: trigger('TP', '9') },
  { a: 9, b: true, p: '5', s: '1', r: false },
];
for (const wire of wires) {
  const raw = JSON.stringify(wire);
  run({ operation: 'record', raw }, () => {
    const record = cloid.placementRecordFromWire(JSON.parse(raw), 'BTC-PERP', 1700000000000);
    return { record: { ...record, price: bits(record.price), amount: bits(record.amount), timeInForce: record.timeInForce ?? null } };
  });
}
assert.equal(cases.length, 34, 'cloid fixed source case inventory');
if (process.argv.includes('--binding-audit')) {
  const inputs = JSON.parse(fs.readFileSync(0, 'utf8'));
  assert.equal(inputs.length, 12, 'constructed native wire cohort');
  const outsideDomain = { ...inputs[0].wire, r: 'outside-placement-domain' };
  assert.equal(cloid.placementRecordFromWire(outsideDomain, 'BTC-PERP', 0).reduceOnly,
    'outside-placement-domain', 'source copies a nonboolean r when types are bypassed');
  console.log(JSON.stringify(inputs.map(({ wire, symbol, placedAt }) => {
    assert.equal(typeof wire.r, 'boolean', 'actual producer boolean r');
    const record = cloid.placementRecordFromWire(wire, symbol, placedAt);
    return { ...record, price: bits(record.price), amount: bits(record.amount), placedAt: bits(record.placedAt), timeInForce: record.timeInForce ?? null };
  })));
} else {
  console.log(JSON.stringify({ source: 'actual hyperliquid.cloid, clientOrderId and order-inference modules with actual shared enums', cases }));
}
