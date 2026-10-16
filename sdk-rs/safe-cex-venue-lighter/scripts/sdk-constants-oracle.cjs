const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/lighter-sdk/src/constants/index.ts');
let source = fs.readFileSync(file, 'utf8');
const mutation = {
  chain: ['ArbitrumOne: 42161', 'ArbitrumOne: 42162'],
  version: ["export const SdkVersion = '1.0.0'", "export const SdkVersion = '1.0.1'"],
  bigint: ["BigInt('0xffffffffffffffff')", "BigInt('0xfffffffffffffffe')"],
}[process.env.LIGHTER_SDK_CONSTANT_MUTATION];
if (mutation) {
  assert.equal(source.split(mutation[0]).length, 2, 'SDK constant mutation anchor');
  source = source.replace(...mutation);
}
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const declarations = tree.statements.filter(n => ts.isVariableStatement(n) && n.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword));
const names = declarations.flatMap(n => n.declarationList.declarations.map(d => {
  assert.ok(ts.isIdentifier(d.name), 'SDK constant declaration identifier'); return d.name.text;
}));
assert.equal(new Set(names).size, names.length, 'unique SDK constant exports');
const exportsObject = {};
vm.runInNewContext(ts.transpileModule(declarations.map(n => n.getText(tree)).join('\n'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText, { exports: exportsObject });
function encode(value) {
  if (typeof value === 'bigint') return { $bigint: value.toString() };
  if (typeof value === 'number') { const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value); return { $number: bytes.toString('hex') }; }
  if (typeof value === 'string') return value;
  assert.ok(value && typeof value === 'object' && !Array.isArray(value), 'constant value domain');
  return { $table: Object.entries(value).map(([k, v]) => [k, encode(v)]) };
}
process.stdout.write(JSON.stringify(Object.fromEntries(names.map(name => [name, encode(exportsObject[name])]))));
