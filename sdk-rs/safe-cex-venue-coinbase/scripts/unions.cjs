// Accepted literal sets come from live TS, independently of Rust serde attributes.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const {indexedStringLiterals}=require('./literal-tuples.cjs');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase');
const legacyName=s=>s.toLowerCase().replace(/(^|_)(\w)/g,(_,a,b)=>b.toUpperCase());
const pascal=s=>s.replace(/(^|[-_ ])(\w)/g,(_,a,b)=>b.toUpperCase());
function inventory(file,legacy){
 const source=ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);
 const aliases=new Map(source.statements.filter(ts.isTypeAliasDeclaration).map(n=>[n.name.text,n]));
 function literals(n){
  if(ts.isTypeReferenceNode(n))return literals(aliases.get(n.typeName.getText(source)).type);
  if(ts.isIndexedAccessTypeNode(n))return indexedStringLiterals(ts,source,n).values;
  if(ts.isUnionTypeNode(n))return n.types.flatMap(literals);
  assert.ok(ts.isLiteralTypeNode(n),`unsupported literal ${n.getText(source)}`);
  if(n.literal.kind===ts.SyntaxKind.FalseKeyword)return [false];
  if(n.literal.kind===ts.SyntaxKind.TrueKeyword)return [true];
  assert.ok(ts.isStringLiteral(n.literal));return [n.literal.text];
 }
 const found={},records={};
 function sample(n,fieldIndex=0){
  if(n.kind===ts.SyntaxKind.StringKeyword)return `sample-${fieldIndex}`;
  if(n.kind===ts.SyntaxKind.NumberKeyword)return 1.25+fieldIndex;
  if(n.kind===ts.SyntaxKind.BooleanKeyword)return fieldIndex%2===0;
  if(ts.isUnionTypeNode(n)||ts.isLiteralTypeNode(n)||ts.isIndexedAccessTypeNode(n))return literals(n)[fieldIndex%literals(n).length];
  if(ts.isTypeReferenceNode(n))return n.typeName.getText(source)==='Array'?[sample(n.typeArguments[0],fieldIndex+1)]:sample(aliases.get(n.typeName.getText(source)).type,fieldIndex);
  if(ts.isArrayTypeNode(n))return [sample(n.elementType,fieldIndex+1)];
  assert.ok(ts.isTypeLiteralNode(n));return Object.fromEntries(n.members.map((f,index)=>[f.name.text,sample(f.type,index+1)]));
 }
 function visit(n,name){
  if(ts.isUnionTypeNode(n)||ts.isLiteralTypeNode(n)||ts.isIndexedAccessTypeNode(n)){
   const values=literals(n);
   const key=values.length===1&&values[0]===false?'CoinbaseStarbaseFalse':name;
   if(Object.hasOwn(found,key))assert.deepEqual(found[key],values);else found[key]=values;
  }else if(ts.isTypeReferenceNode(n)){
   if(n.typeName.getText(source)==='Array')visit(n.typeArguments[0],name+'Item');
   // Named aliases are visited separately, retaining their actual Rust names.
   else assert.ok(aliases.has(n.typeName.getText(source)));
  }else if(ts.isArrayTypeNode(n))visit(n.elementType,name+'Item');
  else if(ts.isTypeLiteralNode(n)){records[name]=sample(n);for(const f of n.members)visit(f.type,name+(legacy?legacyName(f.name.text):pascal(f.name.text)));}
  else assert.ok([ts.SyntaxKind.StringKeyword,ts.SyntaxKind.NumberKeyword,ts.SyntaxKind.BooleanKeyword].includes(n.kind),`unsupported type ${n.getText(source)}`);
 }
 for(const [name,n]of aliases)visit(n.type,legacy?legacyName(name):name);
 return {unions:found,records};
}
const legacy=inventory('coinbase.types.ts',true),cutover=inventory('coinbase.starbase-cutover.ts',false);
process.stdout.write(JSON.stringify({legacy:legacy.unions,cutover:cutover.unions,legacyRecords:legacy.records,cutoverRecords:cutover.records}));
