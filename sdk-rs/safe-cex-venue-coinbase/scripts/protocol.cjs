const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const dir = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase');
const tables = {};
for (const file of fs.readdirSync(dir).filter(f => /^coinbase\.starbase.*\.ts$/.test(f)).sort()) {
  const source = ts.createSourceFile(file, fs.readFileSync(path.join(dir, file), 'utf8'), ts.ScriptTarget.Latest, true);
  // Evaluate only exported protocol data, never constructors or side effects.
  const statements = source.statements.filter(s => ts.isVariableStatement(s)
    && s.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword)
    && s.declarationList.declarations.every(d => /^[A-Z][A-Z0-9_]*$/.test(d.name.getText(source))));
  const exports = {};
  vm.runInNewContext(ts.transpileModule(statements.map(s => s.getText(source)).join('\n'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS },
  }).outputText, { exports }, { filename: file });
  tables[file] = exports;
}
function ordered(value) {
  if (Array.isArray(value)) return value.map(ordered);
  if (value !== null && typeof value === 'object') return Object.entries(value).map(([key, v]) => [key, ordered(v)]);
  return value;
}
process.stdout.write(JSON.stringify(Object.fromEntries(Object.entries(tables).map(([file, exports]) => [file,
  Object.fromEntries(Object.entries(exports).map(([name,v]) => [name,ordered(v)])),
]))));
