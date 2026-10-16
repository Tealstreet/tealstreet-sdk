const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');

const sourcePath = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase.ts');
const source = ts.createSourceFile(sourcePath, fs.readFileSync(sourcePath, 'utf8'), ts.ScriptTarget.Latest, true);
const aliases = new Map(source.statements.filter(ts.isTypeAliasDeclaration).map((alias) => [alias.name.text, alias]));
function shape(node) {
  if (node.kind === ts.SyntaxKind.NumberKeyword) return { kind: 'number' };
  if (node.kind === ts.SyntaxKind.StringKeyword) return { kind: 'string' };
  if (node.kind === ts.SyntaxKind.UnknownKeyword) return { kind: 'unknown' };
  if (ts.isLiteralTypeNode(node) && ts.isStringLiteral(node.literal)) return { kind: 'enum', values: [node.literal.text] };
  if (ts.isTypeLiteralNode(node)) {
    return {
      kind: 'object',
      fields: node.members.map((member) => {
        assert.ok(ts.isPropertySignature(member) && member.type && ts.isIdentifier(member.name), 'record members must be named properties');
        return { name: member.name.text, optional: !!member.questionToken, schema: shape(member.type) };
      }),
    };
  }
  if (ts.isTypeReferenceNode(node)) {
    const name = node.typeName.getText(source);
    const alias = aliases.get(name);
    assert.ok(alias, `unknown source type ${name}`);
    return shape(alias.type);
  }
  throw new Error(`unsupported JSON-RPC error response shape: ${node.getText(source)}`);
}
const alias = aliases.get('CoinbaseStarbaseJsonRpcErrorResponse');
assert.ok(alias, 'CoinbaseStarbaseJsonRpcErrorResponse alias must exist');
process.stdout.write(JSON.stringify(shape(alias.type)));
