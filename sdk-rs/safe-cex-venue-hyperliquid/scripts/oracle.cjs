const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const vm = require('node:vm');
const root = path.resolve(__dirname, '../../../');
const source = fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types.ts'),'utf8');
const ast = ts.createSourceFile('types.ts',source,ts.ScriptTarget.Latest,true);
require('./interface-heritage.cjs')(ts,ast);
const runtime = {exports:{}};
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,runtime);
const selections=new Map();
function sample(n,stack=[]) {
 if(selections.has(n)) return sample(selections.get(n),stack);
 if(ts.isParenthesizedTypeNode(n)) return sample(n.type,stack);
 if(ts.isTypeReferenceNode(n)) {
  const name=n.typeName.text ?? n.typeName.getText(ast);if(name==='Array') return [sample(n.typeArguments[0],stack)];
  if(stack.includes(name)) throw Error('recursive '+name);
  const ds=ast.statements.filter(s=>(ts.isTypeAliasDeclaration(s)||ts.isInterfaceDeclaration(s))&&s.name.text===name);
  if(!ds.length) throw Error(name);
  if(ts.isTypeAliasDeclaration(ds[0])) return sample(ds[0].type,[...stack,name]);
  return members(ds.flatMap(d=>[...d.members]),[...stack,name]);
 }
 if(ts.isTypeLiteralNode(n)) return members(n.members,stack);
 if(ts.isArrayTypeNode(n)) return [sample(n.elementType,stack)];
 if(ts.isTupleTypeNode(n)) return n.elements.map(e=>sample(e,stack));
 if(ts.isUnionTypeNode(n)) return sample(n.types.find(t=>t.getText(ast)!=='null'),stack);
 if(n.getText?.(ast)==='null') return null;
 if(ts.isIntersectionTypeNode(n)) { if(n.getText(ast).replace(/\s/g,'')!=='(WsFill[]|WsUserFunding|WsLiquidation|WsNonUserCancel[])&{user:string}') throw Error('review new intersection fixture'); return {...sample(ts.factory.createTypeReferenceNode('WsUserFunding'),stack),user:'wallet'}; }
 if(ts.isLiteralTypeNode(n)) return n.literal.kind===ts.SyntaxKind.NullKeyword?null:n.literal.text;
 if(n.kind===ts.SyntaxKind.StringKeyword) return 'wire-雪\u0000';
 if(n.kind===ts.SyntaxKind.NumberKeyword) return 17;
 if(n.kind===ts.SyntaxKind.BooleanKeyword) return true;
 if(n.kind===ts.SyntaxKind.AnyKeyword) return {arbitrary:'metadata'};
 throw Error(ts.SyntaxKind[n.kind]);
}
function members(ms,stack){ const o={};for(const m of ms){if(ts.isIndexSignatureDeclaration(m)){o.BTC=sample(m.type,stack)}else {o[m.name.text]=sample(m.type,stack)}}return o;}
const notModeled=require('./not-modeled-types.cjs');
const names=[...new Set(ast.statements.filter(s=>ts.isInterfaceDeclaration(s)||ts.isTypeAliasDeclaration(s)).map(s=>s.name.text))].filter(name=>!notModeled.has(name));
const cases=[];
for(const name of names) {
 const rootType=ts.factory.createTypeReferenceNode(name);
 const add=(edge)=>cases.push({name,edge,raw:JSON.stringify(sample(rootType))});
 selections.clear();add('base');
 function edges(n,label){
  if(ts.isTypeReferenceNode(n)){const id=n.typeName.text; for(const d of ast.statements.filter(s=>(ts.isTypeAliasDeclaration(s)||ts.isInterfaceDeclaration(s))&&s.name.text===id)){if(d.type)edges(d.type,label);else for(const m of d.members){if(m.type)edges(m.type,label+'.'+(m.name?.text??'index'))}}return;}
  if(ts.isUnionTypeNode(n)){
   for(const t of n.types){selections.set(n,t);add(label+':'+t.getText(ast));edges(t,label);selections.delete(n);}
  } else if(ts.isTypeLiteralNode(n)){for(const m of n.members)if(m.type)edges(m.type,label+'.'+(m.name?.text??'index'));}
  else if(ts.isArrayTypeNode(n))edges(n.elementType,label+'[]');
  else if(ts.isTupleTypeNode(n))n.elements.forEach((e,i)=>edges(e,label+'['+i+']'));
  else if(ts.isParenthesizedTypeNode(n))edges(n.type,label);
 }
 edges(rootType,name);
}

const corpus=JSON.stringify({tables:runtime.exports,cases},null,2)+'\n';
const fixturePath=path.join(__dirname,'../tests/fixtures/types.json');
if(process.argv.includes('--check')) {if(fs.readFileSync(fixturePath,'utf8')!==corpus) throw Error('live TS type/table oracle drift');}
else fs.writeFileSync(fixturePath,corpus);
