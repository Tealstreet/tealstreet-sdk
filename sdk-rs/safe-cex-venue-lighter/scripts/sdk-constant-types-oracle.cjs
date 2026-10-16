const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/lighter-sdk/src/constants/index.ts');
let source = fs.readFileSync(file, 'utf8');
const mutation = process.env.LIGHTER_SDK_ALIAS_MUTATION;
if (mutation === 'missing-reexport') {
  const anchor = "export * from '../types/constants';";
  assert.equal(source.split(anchor).length, 2, 'missing-reexport exact source anchor');
  source = source.replace(anchor, "export * from '../types/__missing_constants__';");
}
if (mutation === 'widen') source = source.replace('} as const;', '};');
if (mutation === 'literal') source = source.replace("NetworkError: 'NETWORK_ERROR'", "NetworkError: 'CHANGED_ERROR'");
const host = ts.createCompilerHost({});
const read = host.readFile.bind(host);
host.readFile = name => name === file ? source : read(name);
const program = ts.createProgram([file], { target: ts.ScriptTarget.ES2022, skipLibCheck: true }, host);
const tree = program.getSourceFile(file);
const checker = program.getTypeChecker();
const result = {};
for (const node of tree.statements.filter(ts.isTypeAliasDeclaration)) {
  const name = node.name.text;
  const type = checker.getTypeFromTypeNode(node.type);
  assert.ok(type.isUnion(), `${name} must remain a closed literal union`);
  result[name] = type.types.map(member => {
    assert.ok(member.flags & (ts.TypeFlags.StringLiteral | ts.TypeFlags.NumberLiteral), `${name} literal member`);
    return member.value;
  });
}
// Preserve checker-derived literal order before forcing all program diagnostics.
const diagnostics = ts.getPreEmitDiagnostics(program);
assert.equal(diagnostics.length, 0, `constant alias source program diagnostics must be empty: ${diagnostics.length}\n${ts.formatDiagnostics(diagnostics, {getCanonicalFileName:file=>file, getCurrentDirectory:()=>path.dirname(file), getNewLine:()=> '\n'})}`);
process.stdout.write(JSON.stringify(result));
