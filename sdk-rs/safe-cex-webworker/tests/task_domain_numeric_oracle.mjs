import fs from 'node:fs';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url),nodes=new Map();
for(const path of ['types.ts','webworker/chaserOrder/ChaserOrderOptions.ts','webworker/twap/TwapOptions.ts','webworker/twap/TwapState.ts','webworker/twap/TwapStatus.ts']){
 let text=fs.readFileSync(new URL(path,root),'utf8');
 if(path==='webworker/twap/TwapOptions.ts'&&process.argv.includes('--unsupported-type'))text=text.replace('size: number','size: unknown');
 if(path==='webworker/twap/TwapOptions.ts'&&process.argv.includes('--inherited-number'))text=text.replace('export interface TwapOptions {','interface ExtraNumeric { hiddenNumeric: number; }\nexport interface TwapOptions extends ExtraNumeric {');
 if(path==='webworker/twap/TwapOptions.ts'&&process.argv.includes('--extra-number'))text=text.replace('size: number','additionalNumeric: number; size: number');
 const ast=ts.createSourceFile(path,text,ts.ScriptTarget.Latest,true);
 for(const node of ast.statements)if(node.name)nodes.set(node.name.text,node);
}
function shape(node,path,fields){
 if(ts.isTypeAliasDeclaration(node))return shape(node.type,path,fields);
 if(ts.isTypeReferenceNode(node)){const target=nodes.get(node.typeName.getText());assert.ok(target,'TASK-DOMAIN resolved reference '+node.getText());return shape(target,path,fields);}
 if(ts.isParenthesizedTypeNode(node))return shape(node.type,path,fields);
 if(node.kind===ts.SyntaxKind.NumberKeyword){fields.push(path);return 1;}
 if(node.kind===ts.SyntaxKind.StringKeyword)return 'x';
 if(node.kind===ts.SyntaxKind.BooleanKeyword)return true;
 if(ts.isLiteralTypeNode(node)){assert.ok(ts.isStringLiteral(node.literal),'TASK-DOMAIN string literal');return node.literal.text;}
 if(ts.isUnionTypeNode(node)){for(const arm of node.types)assert.ok(ts.isLiteralTypeNode(arm)&&ts.isStringLiteral(arm.literal),'TASK-DOMAIN closed string union');return shape(node.types[0],path,fields);}
 if(ts.isEnumDeclaration(node)){assert.ok(node.members.every(m=>m.initializer&&ts.isStringLiteral(m.initializer)),'TASK-DOMAIN string enum');return node.members[0].initializer.text;}
 if(ts.isArrayTypeNode(node))return [shape(node.elementType,path+'/0',fields)];
 if(ts.isInterfaceDeclaration(node))assert.ok(!node.heritageClauses?.length,'TASK-DOMAIN unsupported interface heritage');
 if(ts.isTypeLiteralNode(node)||ts.isInterfaceDeclaration(node))return Object.fromEntries(node.members.map(m=>{assert.ok(ts.isPropertySignature(m)&&m.type,'TASK-DOMAIN property');const key=m.name.getText().replace(/^['"]|['"]$/g,'');return [key,shape(m.type,path+'/'+key,fields)];}));
 if(node.kind===ts.SyntaxKind.AnyKeyword){assert.equal(path,'/activeMarket/info','TASK-DOMAIN only named Market.info passthrough');fields.push(path+'/nested/0');return {nested:[1]};}
 throw new Error('TASK-DOMAIN unsupported shape '+node.getText());
}
const records={};
for(const name of ['ChaserOrderOptions','TwapOptions','TwapState']){const fields=[];const sample=shape(nodes.get(name),'',fields);records[name]={fields,sample};}
assert.deepEqual(Object.values(records).map(r=>r.fields.length),[5,19,20],'TASK-DOMAIN complete numeric graph inventory');
const bytes=new DataView(new ArrayBuffer(8));
const numbers=Object.fromEntries(['-0','-0.0','0','1e400','-1e400','1e-400','-1e-400','1.25','1e3','-1e-4000'].map(token=>{bytes.setFloat64(0,JSON.parse(token));return [token,bytes.getBigUint64(0).toString(16).padStart(16,'0')];}));
process.stdout.write(JSON.stringify({records,numbers}));
