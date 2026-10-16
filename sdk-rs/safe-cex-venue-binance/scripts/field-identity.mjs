// Inventory and distinguish every actual wire field, including optional siblings.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/exchanges/binance/',import.meta.url),cases=[];
for(const variant of ['binance','binancepm']){
 const file=`${variant}.types.ts`,ast=ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
 assert.equal(ast.statements.filter(ts.isInterfaceDeclaration).length,0,'field identity requires explicit type literals, not inherited interfaces');
 const declarations=ast.statements.filter(ts.isTypeAliasDeclaration);
 for(const declaration of declarations){
  const type=ts.isArrayTypeNode(declaration.type)?declaration.type.elementType:declaration.type;
  if(ts.isUnionTypeNode(type))continue;
  assert.ok(ts.isTypeLiteralNode(type),'field identity refuses unclassified record shape');
  const record={};let index=0;
  for(const field of type.members){
   assert.ok(ts.isPropertySignature(field)&&field.type,'field identity requires named properties');
   const key=field.name.text;index++;
   if(field.type.kind===ts.SyntaxKind.StringKeyword)record[key]=`${key}:value-${index}`;
   else if(field.type.kind===ts.SyntaxKind.NumberKeyword)record[key]=100.5+index;
   else if(field.type.kind===ts.SyntaxKind.BooleanKeyword)record[key]=true;
   else{const alias=declarations.find(x=>x.name.text===field.type.typeName?.getText(ast));assert.ok(alias&&ts.isUnionTypeNode(alias.type));record[key]=alias.type.types[0].literal.text;}
  }
  cases.push({variant,record:declaration.name.text,raw:JSON.stringify(record),expected:record});
 }
}
assert.equal(cases.length,4,'four public wire records');
process.stdout.write(JSON.stringify(cases));
