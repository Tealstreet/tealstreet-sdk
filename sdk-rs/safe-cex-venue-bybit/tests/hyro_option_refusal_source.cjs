// Execute maintained inherited option methods with HyroTrader's actual guard.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = process.argv[2];
const ts = require(path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'node_modules/typescript')));
function members(relative, className, names) {
  const file = path.join(root, 'packages/safe-cex/src/exchanges/bybit', relative);
  const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const cls = source.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === className);
  assert.ok(cls, className);
  return names.map(name => {
    const found = cls.members.filter(node => node.name?.getText(source) === name);
    assert.equal(found.length, 1, name);
    return found[0].getText(source);
  }).join('\n');
}
(async () => {
  const body = members('bybit.exchange.ts', 'BybitExchange', ['fetchOptionChain', 'fetchOptionExpiries', 'fetchAccountGreeks', 'assertOptionsEnabled']) + '\n' + members('hyrotrader.exchange.ts', 'HyrotraderExchange', ['optionsEnabled']);
  const code = ts.transpileModule(`class Source { ${body} }; Source;`, { compilerOptions: { target: ts.ScriptTarget.ES2022 }}).outputText;
  const Source = vm.runInNewContext(code);
  const venue = new Source();
  let calls = 0;
  venue.xhr = { get() { calls++; throw Error('unexpected option HTTP'); } };
  const rows = [];
  for (const [name, args] of [['fetchOptionChain', [{ underlying: 'BTC' }]], ['fetchOptionExpiries', ['BTC']], ['fetchAccountGreeks', []]]) {
    try { await venue[name](...args); throw Error('expected disabled options'); }
    catch (error) { rows.push({ method: name, name: error.name, message: error.message }); }
  }
  assert.equal(calls, 0);
  assert.equal(rows.length, 3);
  for (const row of rows) assert.equal(row.message, 'Options are not enabled on this Bybit account');
  process.stdout.write(JSON.stringify({ rows, calls }));
})().catch(error => { console.error(error); process.exitCode = 1; });
