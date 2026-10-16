const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/lighter-sdk/src/types/constants.ts');
let source = fs.readFileSync(file, 'utf8');
const mutation = {
  tx: ['L2CreateOrder: 14', 'L2CreateOrder: 15'],
  gas: ['DefaultGasPriceMultiplier = 1.2', 'DefaultGasPriceMultiplier = 1.3'],
  size: ['18446744073709551615', '18446744073709551614'],
}[process.env.LIGHTER_SDK_PROTOCOL_MUTATION];
if (mutation) {
  assert.equal(source.split(mutation[0]).length, 2, 'SDK protocol mutation anchor');
  source = source.replace(...mutation);
}
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const declarations = tree.statements.filter(n => ts.isVariableStatement(n) && n.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword));
const names = declarations.flatMap(n => n.declarationList.declarations.map(d => {
  assert.ok(ts.isIdentifier(d.name), 'SDK protocol declaration identifier'); return d.name.text;
}));
assert.equal(new Set(names).size, names.length, 'unique SDK protocol exports');
const exportsObject = {};
vm.runInNewContext(ts.transpileModule(declarations.map(n => n.getText(tree)).join('\n'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText, { exports: exportsObject });
function encode(value) {
  if (typeof value === 'bigint') return { $bigint: value.toString() };
  if (typeof value === 'number') { const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value); return { $number: bytes.toString('hex') }; }
  if (typeof value === 'string') return value;
  assert.ok(value && typeof value === 'object' && !Array.isArray(value), 'protocol value domain');
  return { $table: Object.entries(value).map(([k, v]) => [k, encode(v)]) };
}
process.stdout.write(JSON.stringify(Object.fromEntries(names.map(name => [name, encode(exportsObject[name])]))));
