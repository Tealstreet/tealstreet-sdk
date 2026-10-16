// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
// Field-identity supplement; original task corpora remain unchanged. Capture host: process.arch.
import fs from 'node:fs';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url),nodes=new Map();
for(const path of ['types.ts','webworker/chaserOrder/ChaserOrderOptions.ts','webworker/chaserOrder/ChaserOrderState.ts','webworker/twap/TwapOptions.ts','webworker/twap/TwapState.ts','webworker/twap/TwapStatus.ts']){
 const ast=ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);
 for(const node of ast.statements)if(node.name)nodes.set(node.name.text,node);
}
let ordinal=0;
function shape(node,path,leaves){
 assert.ok(node,'IDENTITY resolved source type '+path);
 if(ts.isTypeAliasDeclaration(node)||ts.isParenthesizedTypeNode(node))return shape(node.type,path,leaves);
 if(ts.isTypeReferenceNode(node))return shape(nodes.get(node.typeName.getText()),path,leaves);
 const id=++ordinal;
 let values;
 if(node.kind===ts.SyntaxKind.StringKeyword)values=['identity:'+path];
 else if(node.kind===ts.SyntaxKind.NumberKeyword)values=[id+0.25];
 else if(node.kind===ts.SyntaxKind.BooleanKeyword)values=[false,true];
 else if(ts.isUnionTypeNode(node))values=node.types.map(t=>{assert.ok(ts.isLiteralTypeNode(t)&&ts.isStringLiteral(t.literal),'IDENTITY closed union');return t.literal.text;});
 else if(ts.isEnumDeclaration(node))values=node.members.map(m=>{assert.ok(ts.isStringLiteral(m.initializer),'IDENTITY string enum');return m.initializer.text;});
 if(values){leaves.push({path,values});return values[0];}
 if(ts.isArrayTypeNode(node))return [shape(node.elementType,path+'/0',leaves)];
 if(node.kind===ts.SyntaxKind.AnyKeyword){assert.equal(path,'/activeMarket/info','IDENTITY named passthrough');return {retained:'info-identity'};}
 if(ts.isInterfaceDeclaration(node))assert.ok(!node.heritageClauses?.length,'IDENTITY unsupported heritage');
 assert.ok(ts.isInterfaceDeclaration(node)||ts.isTypeLiteralNode(node),'IDENTITY unsupported '+node.getText());
 return Object.fromEntries(node.members.map(m=>{assert.ok(ts.isPropertySignature(m)&&m.type,'IDENTITY property');const key=m.name.getText().replace(/^['"]|['"]$/g,'');return [key,shape(m.type,path+'/'+key,leaves)];}));
}
export const records={};
for(const name of ['ChaserTerminate','ChaserOrderOptions','ChaserCompleteEvent','ChaserOrderState','TwapOptions','TwapState']){
 const leaves=[],sample=shape(nodes.get(name),'',leaves),cases=[];
 for(const {path,values} of leaves)for(const value of values){const input=structuredClone(sample),keys=path.slice(1).split('/');let target=input;for(const key of keys.slice(0,-1))target=target[key];target[keys.at(-1)]=value;cases.push({path,input});}
 records[name]={leaves:leaves.map(x=>x.path),cases};
}
if(process.argv[1] && pathToFileURL(process.argv[1]).href===import.meta.url)process.stdout.write(JSON.stringify({label:'field-identity supplement; no retroactive independence claim',architecture:`${process.platform}/${process.arch}`,nodeMajor:Number(process.versions.node.split('.')[0]),records}));
