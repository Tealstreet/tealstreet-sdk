const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/base.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
assert.equal(tree.parseDiagnostics.length, 0);
const base = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BaseExchange');
const names = ['fetchAndSetOrders', 'fetchAndSetPrivateSnapshot', 'getOrFetchPositionsForSymbol'];
const members = names.map(name => {
  const member = base.members.find(n => n.name?.getText(tree) === name && n.initializer);
  assert.ok(member, name);
  assert.ok(member.initializer.modifiers?.some(m => m.kind === ts.SyntaxKind.AsyncKeyword), name);
  return member.getText(tree);
});
const scope = { exchangeMetadata: {} };
vm.runInNewContext(ts.transpileModule(`class Source { ${members.join('\n')} } globalThis.Source=Source;`, {
  compilerOptions: { target: ts.ScriptTarget.ES2022 },
}).outputText, scope);
(async () => {
  const rows = [];
  for (const method of names) {
    const receiver = new scope.Source();
    const cause = { witness: method };
    receiver.isPublicDataOnly = false;
    receiver.isDisposed = false;
    Object.defineProperty(receiver, 'fetchOrders', { get() { throw cause; } });
    Object.defineProperty(receiver, 'store', { get() { throw cause; } });
    const argument = { valueOf() { throw cause; } };
    let result;
    let syncThrow = false;
    try { result = receiver[method](argument); } catch { syncThrow = true; }
    const promise = result != null && typeof result.then === 'function';
    let caught = false;
    if (promise) await result.catch(error => { caught = error === cause; });
    rows.push({ method, syncThrow, promise, caught });
  }
  const guarded = [];
  for (const kind of ['public', 'disposed']) {
    const receiver = new scope.Source();
    receiver.isPublicDataOnly = kind === 'public';
    receiver.isDisposed = kind === 'disposed';
    let coerced = false;
    await receiver.fetchAndSetPrivateSnapshot({ valueOf() { coerced = true; throw Error('unreached'); } });
    guarded.push({ kind, coerced });
  }
  process.stdout.write(JSON.stringify({ rows, guarded }));
})().catch(error => { console.error(error); process.exitCode = 1; });
