const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/lighter-sdk/src/types/constants.ts');
let source = fs.readFileSync(file, 'utf8');
const mutation = process.env.LIGHTER_SDK_PROTOCOL_TYPE_MUTATION;
if (mutation === 'widen') source = source.replace('} as const;', '};');
if (mutation === 'numeric') source = source.replace('Limit = 0', 'Limit = 12');
if (mutation === 'string') source = source.replace('Subscribe = "subscribe"', 'Subscribe = "changed"');
const host = ts.createCompilerHost({});
const read = host.readFile.bind(host);
host.readFile = name => name === file ? source : read(name);
const program = ts.createProgram([file], { target: ts.ScriptTarget.ES2022, skipLibCheck: true }, host);
const tree = program.getSourceFile(file);
assert.equal(program.getSyntacticDiagnostics(tree).length, 0, 'protocol type syntax');
const checker = program.getTypeChecker();
const result = {};
for (const node of tree.statements) {
  if (ts.isTypeAliasDeclaration(node)) {
    const type = checker.getTypeFromTypeNode(node.type);
    assert.ok(type.isUnion(), `${node.name.text} must remain a closed literal union`);
    result[node.name.text] = { literals: type.types.map(member => {
      assert.ok(member.flags & ts.TypeFlags.NumberLiteral, 'TxTypeValue numeric literal');
      return member.value;
    }) };
  } else if (ts.isEnumDeclaration(node)) {
    const type = checker.getTypeAtLocation(node.name);
    result[node.name.text] = {
      numberAssignable: checker.isTypeAssignableTo(checker.getNumberType(), type),
      members: node.members.map(member => [member.name.text, checker.getConstantValue(member)]),
    };
    assert.ok(result[node.name.text].members.every(([,v]) => typeof v === 'number' || typeof v === 'string'), 'constant enum members');
  }
}
for (const data of Object.values(result)) {
  if (data.numberAssignable) data.members = data.members.map(([key, value]) => {
    const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
    return [key, { $number: bytes.toString('hex') }];
  });
}
const overflow = ['1e400', '-1e400', '-0', '-0.0', '0'].map(raw => {
  const value = JSON.parse(raw); const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
  return { raw, bits: bytes.toString('hex') };
});
process.stdout.write(JSON.stringify({ types: result, overflow }));
