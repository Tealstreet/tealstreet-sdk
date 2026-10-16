const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
const member = cls.members.find(n => n.name?.getText(ast) === 'afterMethodCalledInWorker');
if (!member) throw Error('Missing afterMethodCalledInWorker');
let printed = ts.createPrinter().printNode(ts.EmitHint.Unspecified, member, ast);
if (process.env.ASTERDEX_WORKER_CONTROL === 'invert') {
  const original = 'Boolean(args[0])';
  if (!printed.includes(original)) throw Error('Worker hook control source missing');
  printed = printed.replace(original, '!Boolean(args[0])');
}
const compiled = ts.transpileModule(`export class Oracle { store:any;\n${printed}\n}`, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText;
const loaded = { exports: {} };
new Function('exports', compiled)(loaded.exports);
const oracle = new loaded.exports.Oracle();
const values = [undefined, null, false, true, 0, -0, 1, '', 'false', [], {}, [0]];
const cases = [];
for (const method of ['changePositionMode', 'fetchBalance']) {
  for (const args of [[], ...values.map(value => [value])]) {
    const calls = [];
    oracle.store = { setSetting: (...values) => calls.push(values) };
    oracle.afterMethodCalledInWorker(undefined, method, ...args);
    cases.push({
      method,
      args: args.map(value => value === undefined ? { undefined: true } : value),
      calls,
    });
  }
}
process.stdout.write(JSON.stringify({ cases }));
