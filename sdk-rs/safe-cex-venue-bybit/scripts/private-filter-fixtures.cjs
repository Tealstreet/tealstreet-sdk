const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/bybit/bybit.ws-private.ts');
const source = fs.readFileSync(file, 'utf8');
const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'BybitPrivateWebsocket');
const selected = cls.members.filter(node => node.name?.getText(ast) === 'rowsThisConnectionTrades');
assert.equal(selected.length, 1, 'exact maintained member');
const context = vm.createContext({});
vm.runInContext(ts.transpileModule(`class Subject { ${selected[0].getText(ast)} } globalThis.Subject = Subject`, {
  compilerOptions: { target: ts.ScriptTarget.ES2022 },
}).outputText, context);
const profiles = [];
for (const profile of ['replace', 'delete', 'shrink', 'append', 'sparse', 'ordinary', 'selector-false']) {
  const api = new context.Subject();
  const first = { symbol: 'first' };
  const second = { symbol: 'second' };
  const replacement = { symbol: 'replacement' };
  const data = profile === 'sparse' ? [first, , second] : [first, second];
  const calls = [];
  api.parent = { tradesCategoryOf(symbol) {
    calls.push(symbol);
    if (symbol === 'first') {
      if (profile === 'replace') data[1] = replacement;
      if (profile === 'delete') delete data[1];
      if (profile === 'shrink') data.length = 0;
      if (profile === 'append') data.push(replacement);
    }
    return profile !== 'selector-false' || symbol === 'second';
  }};
  const rows = api.rowsThisConnectionTrades(data);
  const expected = profile === 'selector-false' ? [second] : profile === 'replace' ? [first, replacement]
    : ['delete', 'shrink'].includes(profile) ? [first] : [first, second];
  assert.deepEqual(calls, profile === 'selector-false' ? ['first', 'second'] : expected.map(row => row.symbol), profile + ' callback order');
  assert.equal(rows.length, expected.length, profile + ' count');
  rows.forEach((row, index) => assert.equal(row, expected[index], profile + ' original row identity'));
  profiles.push({ profile, calls, rows: rows.map(row => row.symbol) });
}
console.log(JSON.stringify(profiles));
