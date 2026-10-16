const fs = require('node:fs');
const path = require('node:path');
const { ts, root, values } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
const printer = ts.createPrinter();
const member = name => {
  const node = cls.members.find(n => n.name?.getText(ast) === name);
  if (!node) throw Error(`Missing ${name}`);
  return printer.printNode(ts.EmitHint.Unspecified, node, ast);
};
let fetchPositionMode = member('fetchPositionMode');
if (process.env.ASTERDEX_MODE_CONTROL === 'reject-false') {
  const expected = `value === false || value === 'false'`;
  if (!fetchPositionMode.includes(expected)) throw Error('Position mode source control anchor missing');
  fetchPositionMode = fetchPositionMode.replace(expected, `value === false || value === 'false-control'`);
}
const source = `class Oracle {
  apiVersion = 'v1';
  ${member('getFapiPath')}
  ${fetchPositionMode}
}
export { Oracle };`;
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const loaded = { exports: {} };
new Function('exports', 'getAsterDEXFapiPath', compiled)(loaded.exports, values.getAsterDEXFapiPath);
const { Oracle } = loaded.exports;
const cases = [
  { kind: 'undefined' }, { kind: 'value', data: null },
  ...[true, false, 'true', 'false', 'True', ' false', 1, 0, [], [true], {}, { dualSidePosition: true }, { dualSidePosition: false }, { dualSidePosition: 'true' }, { dualSidePosition: 'false' }, { dualSidePosition: null }, { dualSidePosition: 1 }, { dualSidePosition: 'TRUE', extra: 'x' }]
    .map(data => ({ kind: 'value', data })),
];
const result = async fn => {
  try { return { value: await fn() }; }
  catch (error) { return { error: { name: error.name, message: error.message } }; }
};
(async () => {
  const output = [];
  for (const c of cases) {
    const oracle = new Oracle();
    let requestPath;
    oracle.primaryAPI = { get: async url => { requestPath = url; return { data: c.kind === 'undefined' ? undefined : c.data }; } };
    output.push({ ...c, requestPath: await result(() => oracle.getFapiPath('positionSide/dual')), ...await result(() => oracle.fetchPositionMode()), actualRequestPath: requestPath });
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error => { console.error(error); process.exitCode = 1; });
