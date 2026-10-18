// Field-identity supplements are independent evidence for the retained DTO families.
const identity = process.argv.includes('--field-identity');
// Read the real declarations rather than a second hand-maintained response schema.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const source = ts.createSourceFile('woo.types.ts', fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/woo/woo.types.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
const declarations = new Map(source.statements.filter(ts.isTypeAliasDeclaration).map(s => [s.name.text, s.type]));
let fieldIdentity = 0;
function fixture(type) {
  if (ts.isTypeLiteralNode(type)) return Object.fromEntries(type.members.filter(m => !m.questionToken).map(m => [m.name.text, fixture(m.type)]));
  if (ts.isArrayTypeNode(type)) return [fixture(type.elementType)];
  if (ts.isUnionTypeNode(type)) return fixture(type.types[0]);
  if (ts.isTypeReferenceNode(type)) {
    if (type.typeName.getText(source) === 'Array') return [fixture(type.typeArguments[0])];
    return fixture(declarations.get(type.typeName.getText(source)));
  }
  switch(type.kind) {
    case ts.SyntaxKind.NumberKeyword: return identity ? ++fieldIdentity + 0.25 : 123.25;
    case ts.SyntaxKind.StringKeyword: return identity ? 'field-'+(++fieldIdentity)+'-\uFEFF-\u0085' : 'field-\uFEFF-\u0085';
    case ts.SyntaxKind.BooleanKeyword: return true;
    default: throw Error('Unhandled type: ' + type.getText(source));
  }
}
const cases = [];
for (const [name, type] of declarations) {
  // Erased declarations with no production Rust DTO: real order reads consume raw rows.
  if (name === 'WOOX_ORDER' || name === 'FETCH_ORDER_RESPONSE') continue;
  const base = fixture(type);
  cases.push({name, label:'valid declaration', input:base, accepted:true});
  function visit(t, segments) {
    if(ts.isTypeReferenceNode(t)) {
      if(t.typeName.getText(source)==='Array') return visit(t.typeArguments[0], [...segments,0]);
      return visit(declarations.get(t.typeName.getText(source)),segments);
    }
    if(ts.isArrayTypeNode(t)) return visit(t.elementType,[...segments,0]);
    if(!ts.isTypeLiteralNode(t)) return;
    for(const member of t.members) {
      const key=member.name.text;
      const variants = [['null', null, false], ['missing', undefined, !!member.questionToken]];
      if(identity && member.type.kind===ts.SyntaxKind.BooleanKeyword) variants.push(['field identity false',false,true]);
      if(member.questionToken) variants.push(['present',fixture(member.type),true]);
      if(ts.isUnionTypeNode(member.type)) for(const arm of member.type.types) variants.push(['union '+arm.getText(source),fixture(arm),true]);
      for(const [label,value,accepted] of variants) {
        const input=structuredClone(base);
        const parent=segments.reduce((v,k)=>v[k],input);
        if(value===undefined) delete parent[key]; else parent[key]=value;
        cases.push({name,label:[...segments,key,label].join('.'),input,accepted});
      }
      if(!member.questionToken) visit(member.type,[...segments,key]);
    }
  }
  visit(type,[]);
}
const target=path.join(__dirname,'../tests/fixtures/types.json');
const bytes=JSON.stringify(cases,null,2)+'\n';
if(identity) process.stdout.write(JSON.stringify({capture:{label:"field-identity supplement",platform:process.platform,architecture:process.arch,node:process.version},fixture:cases}));
else if(process.argv.includes('--check')) assert.equal(fs.readFileSync(target,'utf8'),bytes,'WOO type oracle drift');
else fs.writeFileSync(target,bytes);
if(!identity) console.log(`WOO live declaration oracle: ${new Set(cases.map(c => c.name)).size} retained types, ${cases.length} cases`);
