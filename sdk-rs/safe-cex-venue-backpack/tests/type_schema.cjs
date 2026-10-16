const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const sourcePath = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/backpack/backpack.types.ts');
function schema(text) {
  const source = ts.createSourceFile(sourcePath, text, ts.ScriptTarget.Latest, true);
  assert.equal(source.parseDiagnostics.length, 0, 'schema source parses');
  const declarations = new Map(source.statements.filter(ts.isInterfaceDeclaration).map(n => [n.name.text, n]));
  const active = new Set();
  function type(n) {
    if (n.kind === ts.SyntaxKind.StringKeyword) return {kind:'string'};
    if (n.kind === ts.SyntaxKind.NumberKeyword) return {kind:'number'};
    if (n.kind === ts.SyntaxKind.BooleanKeyword) return {kind:'boolean'};
    if (ts.isLiteralTypeNode(n)) {
      if (n.literal.kind === ts.SyntaxKind.NullKeyword) return {kind:'null'};
      assert.ok(ts.isStringLiteral(n.literal), 'supported literal');
      return {kind:'literal', value:n.literal.text};
    }
    if (ts.isUnionTypeNode(n)) return {kind:'union', items:n.types.map(type)};
    if (ts.isArrayTypeNode(n)) return {kind:'array', item:type(n.elementType)};
    if (ts.isTupleTypeNode(n)) return {kind:'tuple', items:n.elements.map(type)};
    if (ts.isTypeLiteralNode(n)) return {kind:'object', fields:members(n.members)};
    if (ts.isTypeReferenceNode(n)) {
      const name=n.typeName.getText(source);
      if (name==='Array') { assert.equal(n.typeArguments.length,1); return {kind:'array',item:type(n.typeArguments[0])}; }
      assert.ok(declarations.has(name)||name==='T', `resolved reference ${name}`);
      assert.ok(!n.typeArguments, 'generic reference must be handled explicitly');
      return {kind:'ref',name};
    }
    throw new Error(`Unhandled source type ${n.getText(source)}`);
  }
  function members(nodes) {
    return nodes.map(n=>{
      assert.ok(ts.isPropertySignature(n)&&ts.isIdentifier(n.name)&&n.type,'property census fail closed');
      return {name:n.name.text, optional:!!n.questionToken, type:type(n.type)};
    });
  }
  function fields(name) {
    assert.ok(!active.has(name), `noncyclic heritage ${name}`); active.add(name);
    const n=declarations.get(name); assert.ok(n, `resolved heritage ${name}`);
    const inherited=(n.heritageClauses??[]).flatMap(h=>h.types.flatMap(t=>{
      assert.ok(!t.typeArguments, 'generic heritage must be handled explicitly');
      return fields(t.expression.getText(source));
    }));
    const all=[...inherited,...members(n.members)];
    assert.equal(new Set(all.map(f=>f.name)).size,all.length,`unambiguous inherited field census ${name}`);
    active.delete(name);return all;
  }
  return Object.fromEntries([...declarations.keys()].map(name=>[name,fields(name)]));
}
if (require.main===module) process.stdout.write(JSON.stringify(schema(fs.readFileSync(sourcePath,'utf8')),null,2));
module.exports={schema,sourcePath,ts};
