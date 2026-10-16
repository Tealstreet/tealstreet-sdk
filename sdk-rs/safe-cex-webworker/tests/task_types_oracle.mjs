import fs from 'node:fs';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url),nodes=new Map();
for(const path of ['types.ts','webworker/StoreWorkerPayload.ts','webworker/ExchangeMetadataWorkerType.ts','webworker/workerErrorSerialization.ts','webworker/chaserOrder/ChaserOrderOptions.ts','webworker/chaserOrder/ChaserOrderState.ts','webworker/twap/TwapOptions.ts','webworker/twap/TwapState.ts','webworker/twap/TwapStatus.ts']){
 let text=fs.readFileSync(new URL(path,root),'utf8');
 // Exercise the inventory with source changes, before parsing, so controls cannot bypass it.
 if(path.endsWith('workerErrorSerialization.ts')&&process.argv.includes('--nullable-parentheses'))text=text.replace('stack?: string','stack?: (string | null)');
 if(path.endsWith('workerErrorSerialization.ts')&&process.argv.includes('--nullable-alias'))text='type NullableStack = (string | null);\n'+text.replace('stack?: string','stack?: NullableStack');
 if(path.endsWith('StoreWorkerPayload.ts')&&process.argv.includes('--shadowed-record'))text='type Record<K extends string,V> = {[P in K]:V} | null;\n'+text;
 const ast=ts.createSourceFile(path,text,ts.ScriptTarget.Latest,true);
 for(const node of ast.statements)if(node.name)nodes.set(node.name.text,node);
 for(const node of ast.statements)if(ts.isVariableStatement(node))for(const d of node.declarationList.declarations)if(d.name.text==='CHASER_KEPT_ORDER_STOP_REASON')nodes.set(d.name.text,d);
}
const market={id:'BTC',symbol:'BTC',base:'BTC',quote:'USD',active:true,precision:{amount:0.01,price:0.01},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}}};
function type(name){const node=nodes.get(name);if(!node)throw new Error('unknown type '+name);return ts.isTypeAliasDeclaration(node)?node.type:node;}
function literals(node){if(ts.isTypeReferenceNode(node))return literals(type(node.typeName.getText()));if(ts.isUnionTypeNode(node))return node.types.flatMap(literals);if(ts.isLiteralTypeNode(node))return [node.literal.text];if(ts.isEnumDeclaration(node))return node.members.map(m=>m.initializer.text);throw new Error('not literal '+node.getText());}
function sample(node){
 if(ts.isTypeReferenceNode(node)){const name=node.typeName.getText();return name==='Market'?market:sample(type(name));}
 if(ts.isArrayTypeNode(node))return [sample(node.elementType)];
 if(ts.isUnionTypeNode(node)||ts.isEnumDeclaration(node))return literals(node)[0];
 if(node.kind===ts.SyntaxKind.StringKeyword)return 'x';if(node.kind===ts.SyntaxKind.NumberKeyword)return -7.1;if(node.kind===ts.SyntaxKind.BooleanKeyword)return true;
 if(ts.isTypeLiteralNode(node)||ts.isInterfaceDeclaration(node))return Object.fromEntries(node.members.map(m=>[m.name.getText().replaceAll("'",''),sample(m.type)]));
 throw new Error('unsupported '+node.getText());
}
const records={};for(const name of ['ChaserTerminate','ChaserOrderOptions','ChaserCompleteEvent','ChaserOrderState','TwapOptions','TwapState']){const node=type(name);records[name]={numericFields:node.members.filter(m=>m.type.kind===ts.SyntaxKind.NumberKeyword).map(m=>m.name.getText()),sample:sample(node),fields:node.members.map(m=>m.name.getText()),optional:node.members.filter(m=>m.questionToken).map(m=>m.name.getText())};}
const unions={};for(const name of ['ChaserOrigin','TwapOrigin','TwapTimeoutAction','TwapFailAction','TwapStatus'])unions[name]=literals(type(name));unions.ChaserTerminateKind=literals(type('ChaserTerminate').members.find(m=>m.name.getText()==='type').type);
function contains(node,kind,seen=new Set()){
 if(ts.isParenthesizedTypeNode(node))return contains(node.type,kind,seen);
 if(ts.isUnionTypeNode(node))return node.types.some(t=>contains(t,kind,seen));
 if(ts.isTypeReferenceNode(node)){
  const name=node.typeName.getText();
  if(name==='Record'){if(nodes.has(name))throw new Error('shadowed nullability builtin Record');return false;}
  if(seen.has(name))throw new Error('recursive nullability alias '+name);
  return contains(type(name),kind,new Set([...seen,name]));
 }
 if(ts.isLiteralTypeNode(node))return node.literal.kind===kind;
 if(ts.isInterfaceDeclaration(node)||ts.isTypeLiteralNode(node)||ts.isEnumDeclaration(node)||ts.isArrayTypeNode(node))return false;
 if([ts.SyntaxKind.StringKeyword,ts.SyntaxKind.NumberKeyword,ts.SyntaxKind.BooleanKeyword,ts.SyntaxKind.AnyKeyword,ts.SyntaxKind.UnknownKeyword,ts.SyntaxKind.UndefinedKeyword,ts.SyntaxKind.NeverKeyword].includes(node.kind))return node.kind===kind;
 throw new Error('unsupported nullability shape '+node.getText());
}
const wireNonnullable=[];
for(const name of ['StoreWorkerPayload','WorkerConfig','PerfFixtureConfig','ExchangeMetadataWorkerType','SerializableWorkerError'])for(const member of type(name).members){
 if((member.questionToken||contains(member.type,ts.SyntaxKind.UndefinedKeyword))&&!contains(member.type,ts.SyntaxKind.NullKeyword)&&!contains(member.type,ts.SyntaxKind.AnyKeyword)&&!contains(member.type,ts.SyntaxKind.UnknownKeyword))wireNonnullable.push(name+'.'+member.name.getText());
}
const ingressBytes=new DataView(new ArrayBuffer(8));
const ingressNumbers=Object.fromEntries(['-0','-0.0','0','1e400','-1e400','1e-400','-1e-400'].map(token=>{ingressBytes.setFloat64(0,JSON.parse(token));return [token,ingressBytes.getBigUint64(0).toString(16).padStart(16,'0')];}));
process.stdout.write(JSON.stringify({records,unions,wireNonnullable,ingressNumbers,kept:nodes.get('CHASER_KEPT_ORDER_STOP_REASON').initializer.text}));
