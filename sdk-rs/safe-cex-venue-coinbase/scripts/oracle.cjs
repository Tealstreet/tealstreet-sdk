// Execute pure TS tables and inspect wire declarations from this checkout only.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const root = path.resolve(__dirname, '../../..');
const venue = 'packages/safe-cex/src/exchanges/coinbase/coinbase.types.ts';
const read = p => fs.readFileSync(path.join(root, p), 'utf8');
const parse = (p, text = read(p)) => ts.createSourceFile(p, text, ts.ScriptTarget.Latest, true);
function evaluate(source, imports = {}) {
  const exports = {};
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
  vm.runInNewContext(js, { exports, require: name => {
    assert.ok(Object.hasOwn(imports, name), `unexpected runtime dependency: ${name}`);
    return imports[name];
  } });
  return exports;
}
const enums = parse('packages/safe-cex/src/types.ts').statements.filter(ts.isEnumDeclaration).map(n => n.getText()).join('\n');
const tables = evaluate(read(venue), { '../../types': evaluate(enums) });
const file = parse(venue);
const aliases = new Map(file.statements.filter(ts.isTypeAliasDeclaration).map(n => [n.name.text, n]));
function schema(node) {
  if (node.kind === ts.SyntaxKind.StringKeyword) return { kind: 'string' };
  if (node.kind === ts.SyntaxKind.NumberKeyword) return { kind: 'number' };
  if (node.kind === ts.SyntaxKind.BooleanKeyword) return { kind: 'boolean' };
  if (ts.isTypeReferenceNode(node)) return schema(aliases.get(node.typeName.getText(file)).type);
  if (ts.isArrayTypeNode(node)) return { kind: 'array', item: schema(node.elementType) };
  if (ts.isUnionTypeNode(node) || ts.isLiteralTypeNode(node)) {
    const values = (node.types ?? [node]).map(n => {
      assert.ok(ts.isLiteralTypeNode(n) && ts.isStringLiteral(n.literal), `unsupported union ${n.getText(file)}`);
      return n.literal.text;
    });
    return { kind: 'enum', values };
  }
  assert.ok(ts.isTypeLiteralNode(node), `unsupported type ${node.getText(file)}`);
  return { kind: 'object', fields: node.members.map(m => ({ name: m.name.text, optional: !!m.questionToken, schema: schema(m.type) })) };
}
const { cases } = require('./wire-cases.cjs');
const types = Object.fromEntries([...aliases].filter(([, n]) => n.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword)).map(([name, n]) => [name, cases(schema(n.type), name)]));
const schemas = Object.fromEntries([...aliases].filter(([, n]) => n.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword)).map(([name, n]) => [name, schema(n.type)]));
const listAccountIdentity = { ...types.LIST_ACCOUNTS_RESPONSE[0].value[0] };
listAccountIdentity.default = false;
listAccountIdentity.ready = true;
const fieldIdentitySupplements = {
  listAccountFlags: {
    label: 'field-identity-supplement: LIST_ACCOUNTS_RESPONSE default and ready',
    value: [listAccountIdentity],
  },
};
const privateWsFile = parse('packages/safe-cex/src/exchanges/coinbase/coinbase.ws-private.ts');
const privateWsClass = privateWsFile.statements.find(statement =>
  ts.isClassDeclaration(statement) && statement.name?.text === 'CoinbasePrivateWebsocket');
function propertyInitializer(name) {
  const member = privateWsClass.members.find(member =>
    ts.isPropertyDeclaration(member) && member.name.getText(privateWsFile) === name);
  assert.ok(member?.initializer, `missing production method ${name}`);
  return member.initializer.getText(privateWsFile);
}
const snapshotHarness = evaluate(`
  const COINBASE_USER_ORDER_SNAPSHOT_BATCH_SIZE = 50;
  class SnapshotHarness {
    constructor(store) { this.store = store; this.snapshotOrdersById = undefined; this.snapshotRawOrderCount = 0; }
    mergeSnapshotOrders = ${propertyInitializer('mergeSnapshotOrders')};
    applyOrderSnapshotBatch = ${propertyInitializer('applyOrderSnapshotBatch')};
    resetOrderSnapshotAssembly = ${propertyInitializer('resetOrderSnapshotAssembly')};
  }
  exports.SnapshotHarness = SnapshotHarness;
`);
function snapshotCase(name, initialOrders, batches, failUpdate = false) {
  const store = {
    orders: initialOrders,
    loaded: { orders: false },
    updates: 0,
    update(changes) {
      this.orders = changes.orders;
      this.loaded = changes.loaded;
      this.updates++;
      if (failUpdate) throw new Error('subscriber failed');
    },
  };
  const subject = new snapshotHarness.SnapshotHarness(store);
  let error = null;
  try {
    for (const batch of batches) subject.applyOrderSnapshotBatch(batch.rawCount, batch.orders);
  } catch (cause) {
    error = cause.message;
  }
  return {
    name,
    initialOrders,
    batches,
    orders: store.orders,
    loaded: store.loaded.orders,
    updates: store.updates,
    pendingRawOrderCount: subject.snapshotRawOrderCount,
    error,
  };
}
const snapshotCases = [
  snapshotCase('current-batch threshold and map identity order', [], [
    { rawCount: 50, orders: [{ id: 'one', price: 10 }, { id: 'two', price: 20 }] },
    { rawCount: 1, orders: [{ id: 'one', price: 11 }] },
  ]),
  snapshotCase('quarantined nonempty snapshot retains state', [{ id: 'old', price: 9 }], [
    { rawCount: 1, orders: [] },
  ]),
  snapshotCase('empty snapshot is authoritative', [{ id: 'old', price: 9 }], [
    { rawCount: 0, orders: [] },
  ]),
  snapshotCase('notification throw follows committed reset', [], [
    { rawCount: 1, orders: [{ id: 'one', price: 10 }] },
  ], true),
];
// Preserve every own table entry and its JS enumeration order, including nested hosts.
function ordered(value) {
  if (Array.isArray(value)) return value.map(ordered);
  if (value !== null && typeof value === 'object') return Object.entries(value).map(([key, v]) => [key, ordered(v)]);
  return value;
}
process.stdout.write(JSON.stringify({ tables: Object.fromEntries(Object.entries(tables).map(([k,v]) => [k,ordered(v)])), types, schemas, fieldIdentitySupplements, snapshotCases }));
