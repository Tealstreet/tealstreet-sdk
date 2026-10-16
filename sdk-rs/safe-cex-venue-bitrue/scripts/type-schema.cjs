// Fail-closed declaration census. Heritage is traversed, never silently omitted.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
function parse(text) {
  const source = ts.createSourceFile('bitrue.types.ts', text, ts.ScriptTarget.Latest, true);
  assert.equal(source.parseDiagnostics.length, 0, 'type census syntax');
  const declarations = new Map(source.statements.filter(n => ts.isTypeAliasDeclaration(n) || ts.isInterfaceDeclaration(n)).map(n => [n.name.text,n]));
  function record(members) {
    return {kind:'object', fields:members.map(m => {
      assert.ok(ts.isPropertySignature(m) && m.type && (ts.isIdentifier(m.name) || ts.isStringLiteral(m.name)), 'unsupported type member');
      return {name:m.name.text, optional:!!m.questionToken, type:walk(m.type)};
    })};
  }
  function walk(n) {
    assert.ok(n, 'unresolved type');
    if(ts.isTypeLiteralNode(n)) return record(n.members);
    if(ts.isArrayTypeNode(n)) return {kind:'array',item:walk(n.elementType)};
    if(ts.isTupleTypeNode(n)) return {kind:'tuple',items:n.elements.map(walk)};
    if(ts.isUnionTypeNode(n)) {
      const arms=n.types.map(walk);
      if(arms.some(t=>t.kind==='string')) {
        assert.ok(arms.every(t=>t.kind==='string'||t.kind==='number'||(t.kind==='literal'&&typeof t.value==='string')), 'unsupported mixed string union');
        return arms.some(t=>t.kind==='number') ? {kind:'number_string'} : {kind:'string'};
      }
      if(arms.every(t=>t.kind==='literal')) return {kind:'enum',values:arms.map(t=>t.value)};
      return {kind:'union',arms};
    }
    if(ts.isLiteralTypeNode(n)) {
      assert.ok(ts.isStringLiteral(n.literal)||ts.isNumericLiteral(n.literal),'unsupported literal');
      return {kind:'literal',value:ts.isNumericLiteral(n.literal)?Number(n.literal.text):n.literal.text};
    }
    if(ts.isTypeReferenceNode(n)) return {kind:'ref',name:n.typeName.getText(source)};
    const kind = new Map([[ts.SyntaxKind.StringKeyword,'string'],[ts.SyntaxKind.NumberKeyword,'number'],[ts.SyntaxKind.BooleanKeyword,'boolean'],[ts.SyntaxKind.AnyKeyword,'any']]).get(n.kind);
    assert.ok(kind, 'unsupported type '+n.getText(source)); return {kind};
  }
  const resolving=new Set(); const result={};
  function declaration(name) {
    if(result[name]) return result[name];
    assert.ok(!resolving.has(name),'recursive declaration'); resolving.add(name);
    const n=declarations.get(name); assert.ok(n,'missing declaration '+name);
    let value;
    if(ts.isInterfaceDeclaration(n)) {
      const fields=[];
      for(const clause of n.heritageClauses||[]) {
        assert.equal(clause.token,ts.SyntaxKind.ExtendsKeyword,'unsupported heritage');
        for(const base of clause.types) {
          assert.ok(!base.typeArguments,'generic heritage unsupported');
          fields.push(...declaration(base.expression.getText(source)).fields);
        }
      }
      for(const field of record(n.members).fields) {
        const prior=fields.findIndex(f=>f.name===field.name);
        if(prior<0) fields.push(field); else fields[prior]=field;
      }
      value={kind:'object',fields};
    } else value=walk(n.type);
    resolving.delete(name); result[name]=value; return value;
  }
  for(const name of declarations.keys()) declaration(name);
  return {declarations:result, exported:[...declarations].filter(([,n])=>n.modifiers?.some(m=>m.kind===ts.SyntaxKind.ExportKeyword)).map(([name])=>name)};
}
function controls() {
  const inherited=parse('interface Parent { inherited: number } interface Child extends Parent { own: string }');
  assert.deepEqual(inherited.declarations.Child.fields.map(f=>f.name),['inherited','own'],'inherited required field census');
  assert.throws(()=>parse('type Mixed = string | boolean'),/unsupported mixed string union/,'mixed union census fails closed');
  assert.throws(()=>parse('type Mixed = number | string | boolean'),/unsupported mixed string union/,'numeric mixed union census fails closed');
  assert.throws(()=>parse('type Unsupported = { [key:string]: number }'),/unsupported type member/,'unsupported census fails closed');
}
controls();
const root=path.resolve(__dirname,'../../..');
const schema=parse(fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.types.ts'),'utf8'));
module.exports={schema,parse,controls};
if(require.main===module) process.stdout.write(JSON.stringify(schema));
