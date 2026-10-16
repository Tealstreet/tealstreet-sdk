// Execute the actual base and Weex socket dispose members, including super.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
function member(file, name) {
  const full = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges', file);
  const tree = ts.createSourceFile(full, fs.readFileSync(full, 'utf8'), ts.ScriptTarget.Latest, true);
  assert.equal(tree.parseDiagnostics.length, 0);
  const cls = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === name);
  const dispose = cls.members.find(n => n.name?.getText(tree) === 'dispose');
  assert.ok(dispose);
  return dispose.getText(tree);
}
const base = member('base.ws.ts', 'BaseWebSocket');
const rows = [];
for (const side of ['public', 'private']) {
  // Resolve the real class name rather than silently accepting absent source.
  const file = `weex/weex.ws-${side}.ts`;
  const full = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges', file);
  const tree = ts.createSourceFile(full, fs.readFileSync(full, 'utf8'), ts.ScriptTarget.Latest, true);
  const cls = tree.statements.find(n => ts.isClassDeclaration(n));
  const derived = member(file, cls.name.text);
  for (const repeated of [false, true]) {
    const events = [];
    const scope = {clearTimeout: value => events.push(['clear', value])};
    const compiled = ts.transpileModule(`class Base {${base}} class Derived extends Base {${derived}} globalThis.Derived=Derived;`, {compilerOptions:{target:ts.ScriptTarget.ES2022}});
    vm.runInNewContext(compiled.outputText, scope);
    const receiver = new scope.Derived();
    receiver.handleActiveTickerSymbolsChanged = () => {};
    receiver.parent = {off(event, callback) {
      assert.equal(callback, receiver.handleActiveTickerSymbolsChanged);
      events.push(['off', event]);
    }};
    receiver.refreshOrdersTimeout = 'orders';
    receiver.refreshPositionsTimeout = 'positions';
    receiver.refreshBalanceTimeout = 'balance';
    receiver.dispose();
    if (repeated) receiver.dispose();
    rows.push({side, repeated, events, forceClosed: receiver.forceClosed});
  }
}
process.stdout.write(JSON.stringify(rows));
