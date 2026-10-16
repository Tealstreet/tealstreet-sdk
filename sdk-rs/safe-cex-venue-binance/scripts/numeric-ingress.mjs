// Numeric ingress for the actual source record schemas; interfaces erase at runtime.
// Node JSON.parse is the runtime boundary, and bits are captured before stringify.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../',import.meta.url);
const lexemes=['-0.0','-0e+1','0.0','1e+0','1e400','-1e400','1e-400','-1e-400','-0','0','1e308','-1e308','0.5','-0.5','5e-324','-5e-324','9007199254740993','1.7976931348623157e308'];
const cases=[];
for(const variant of ['binance','binancepm']){
 const file=`packages/safe-cex/src/exchanges/binance/${variant}.types.ts`;
 const ast=ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
 const declarations=ast.statements.filter(ts.isTypeAliasDeclaration);
 for(const declaration of declarations){
  const type=ts.isArrayTypeNode(declaration.type)?declaration.type.elementType:declaration.type;
  if(!ts.isTypeLiteralNode(type))continue;
  const base={};
  for(const field of type.members){
   assert.ok(ts.isPropertySignature(field));
   if(field.questionToken)continue;
   if(field.type.kind===ts.SyntaxKind.StringKeyword)base[field.name.text]='';
   else if(field.type.kind===ts.SyntaxKind.NumberKeyword)base[field.name.text]=0;
   else if(field.type.kind===ts.SyntaxKind.BooleanKeyword)base[field.name.text]=false;
   else {
    assert.ok(ts.isTypeReferenceNode(field.type),field.getText(ast));
    const alias=declarations.find(x=>x.name.text===field.type.typeName.getText(ast));
    assert.ok(alias&&ts.isUnionTypeNode(alias.type));
    base[field.name.text]=alias.type.types[0].literal.text;
   }
  }
  for(const field of type.members.filter(x=>x.type.kind===ts.SyntaxKind.NumberKeyword))for(const lexeme of lexemes){
   const key=field.name.text,raw=JSON.stringify({...base,[key]:'NUMERIC_LEXEME'}).replace('"NUMERIC_LEXEME"',lexeme);
   const value=JSON.parse(raw)[key],bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);
   assert.equal(typeof value,'number');
   cases.push({variant,record:declaration.name.text,key,lexeme,raw,bits:bytes.toString('hex')});
  }
 }
}
assert.equal(cases.length,162,'nine actual-source numeric fields by eighteen numeric lexemes');
process.stdout.write(JSON.stringify(cases));
