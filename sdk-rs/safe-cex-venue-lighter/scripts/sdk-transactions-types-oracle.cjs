const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file=path.resolve(__dirname,'../../../packages/lighter-sdk/src/types/transactions.ts');
let source=fs.readFileSync(file,'utf8');
const mutation=process.env.LIGHTER_SDK_TRANSACTIONS_MUTATION;
if (mutation === 'missing-enum-imports') {
 const anchor="import { OrderType, TimeInForce, OrderSide, TxTypeValue } from './constants';";
 assert.equal(source.split(anchor).length,2,'missing-enum-imports exact source anchor');
 source=source.replace(anchor,"import { TxTypeValue } from './constants';");
}
if (mutation === 'inherited-number') source = source.replace(/(interface TransactOpts(?:<[^>]+>)?)(\s*\{)/, '$1 extends ExtraNumeric$2') + '\ninterface ExtraNumeric { hiddenNumeric: number; }\n';
if(mutation==='nonce-string') source=source.replace('nonce: number;','nonce: string;');
if(mutation==='order-wrapper') source=source.replace('type: OrderType;','type: OrderSide;');
if(mutation==='gas-required') source=source.replace('gasLimit?: number;','gasLimit: number;');
if(mutation==='l2-wide-tag') source=source.replace("typeof import('./constants').TxType.L2CreateOrder",'TxTypeValue');
if(mutation==='union-member') source=source.replace('  | L2FundingFeeTxInfo;', ';');
if(mutation==='signed-signature-string') source=source.replace('signature: TxSignature;','signature: string;');
if(mutation==='signed-heritage') source=source.replace('interface SignedTx {','interface SignedTx extends ExtraNumeric {')+'\ninterface ExtraNumeric { hiddenNumeric: number; }';
if(mutation==='receipt-status-string') source=source.replace('status: number;','status: string;');
if(mutation==='receipt-heritage') source=source.replace('interface TxReceipt {','interface TxReceipt extends ExtraNumeric {')+'\ninterface ExtraNumeric { hiddenNumeric: number; }';
const constantsFile=path.resolve(__dirname,'../../../packages/lighter-sdk/src/types/constants.ts');
let constants=fs.readFileSync(constantsFile,'utf8');
if(mutation==='lost-const') constants=constants.replace('} as const;','};');
if(mutation==='tag-value') constants=constants.replace('L2CreateOrder: 14','L2CreateOrder: 29');
const host=ts.createCompilerHost({}),read=host.readFile.bind(host);
host.readFile=name=>name===file?source:name===constantsFile?constants:read(name);
const program=ts.createProgram([file],{target:ts.ScriptTarget.ES2022,skipLibCheck:true},host);
const tree=program.getSourceFile(file),checker=program.getTypeChecker();
const tags={},nestedNumbers={},nestedStrings={};
const schemas={},complete={},numeric={},strings={};
const numericKind=t=>['number','OrderType','OrderSide','TimeInForce'].includes(t);
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex')};
for(const name of ['TransactOpts','CreateOrderTxReq','CancelOrderTxReq','TransferTxReq','WithdrawTxReq','UpdateApiKeyTxReq','TxSignature','L2CreateOrderTxInfo','L2CancelOrderTxInfo','L2TransferTxInfo','L2WithdrawTxInfo','L2UpdateApiKeyTxInfo','L2LiquidationTxInfo','L2DeleverageTxInfo','L2FundingFeeTxInfo']) {
 const node=tree.statements.find(n=>ts.isInterfaceDeclaration(n)&&n.name.text===name);assert.ok(node,name);
  assert.ok(!node.heritageClauses?.length, `${name} unsupported interface heritage`);
 schemas[name]=node.members.map(f=>({name:f.name.text,type:f.type.getText(tree),optional:!!f.questionToken}));
 const tag=node.members.find(f=>f.name.text==='txType');
 if(tag){ const type=checker.getTypeFromTypeNode(tag.type);assert.ok(type.flags & ts.TypeFlags.NumberLiteral,`${name}.txType single numeric literal`);tags[name]=type.value; }
 complete[name]=Object.fromEntries(schemas[name].map((f,i)=>[f.name,f.name==='txType'?tags[name]:f.type==='TransactOpts'?complete.TransactOpts:numericKind(f.type)?16777217.25+i:f.type==='boolean'?true:`${name}.${f.name}`]));
 numeric[name]=schemas[name].filter(f=>numericKind(f.type)).flatMap(f=>['-0','-0.0','-0e0','0','1e3','1e400','-1e400','-1e-4000','16777217.25','9007199254740993'].map(raw=>{
  const encoded=JSON.stringify({...complete[name],[f.name]:'__NUMBER__'}).replace('"__NUMBER__"',raw);
  return{field:f.name,raw,encoded,bits:bits(JSON.parse(encoded)[f.name])};
 }));
 strings[name]=['','plain','é','👩','\ufeff','\u0085','\ud800','\udfff','x\ud800y'].map(text=>{
  const value={...complete[name]};for(const f of schemas[name].filter(f=>f.type==='string'))value[f.name]=text;
  const encoded=JSON.stringify(value),parsed=JSON.parse(encoded);
  for(const f of schemas[name].filter(f=>f.type==='string'))assert.equal(parsed[f.name],text);
  return{encoded,units:Array.from({length:text.length},(_,i)=>text.charCodeAt(i)),representable:text.isWellFormed()};
 });
}
for(const name of Object.keys(tags)) {
 nestedNumbers[name]=numeric.TransactOpts.map(w=>({...w,encoded:JSON.stringify({...complete[name],opts:'__OPTS__'}).replace('"__OPTS__"',w.encoded)}));
 nestedStrings[name]=strings.TransactOpts.map(w=>({...w,encoded:JSON.stringify({...complete[name],opts:'__OPTS__'}).replace('"__OPTS__"',w.encoded)}));
}
const union=tree.statements.find(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='L2TxInfo');
assert.ok(union&&ts.isUnionTypeNode(union.type),'L2TxInfo source union');
const variants=union.type.types.map(t=>{assert.ok(ts.isTypeReferenceNode(t),'L2TxInfo named record member');return t.getText(tree);});
const signed=tree.statements.find(n=>ts.isInterfaceDeclaration(n)&&n.name.text==='SignedTx');
assert.ok(signed&&!signed.heritageClauses?.length,'SignedTx unsupported interface heritage');
const signedSchema=signed.members.map(f=>({name:f.name.text,type:f.type.getText(tree),optional:!!f.questionToken}));
const signedCases=variants.map(name=>({name,value:{tx:complete[name],signature:complete.TxSignature,signer:'SignedTx.signer'}}));
const signedNumeric=variants.flatMap(name=>[
 ...numeric[name].map(w=>({...w,name,location:'tx',encoded:JSON.stringify({tx:'__TX__',signature:complete.TxSignature,signer:'SignedTx.signer'}).replace('"__TX__"',w.encoded)})),
 ...nestedNumbers[name].map(w=>({...w,name,location:'opts',encoded:JSON.stringify({tx:'__TX__',signature:complete.TxSignature,signer:'SignedTx.signer'}).replace('"__TX__"',w.encoded)})),
 ...numeric.TxSignature.map(w=>({...w,name,location:'signature',encoded:JSON.stringify({tx:complete[name],signature:'__SIG__',signer:'SignedTx.signer'}).replace('"__SIG__"',w.encoded)})),
]);
const signedStrings=signedCases.flatMap(({name,value})=>['signer','r','s'].flatMap(field=>['','plain','é','👩','\ufeff','\u0085','\ud800','\udfff','x\ud800y'].map(text=>{
 const input=structuredClone(value);if(field==='signer')input.signer=text;else input.signature[field]=text;
 const encoded=JSON.stringify(input),parsed=JSON.parse(encoded);
 assert.equal(field==='signer'?parsed.signer:parsed.signature[field],text);
 return{name,field,encoded,representable:text.isWellFormed()};
})));
const signedDepth=signedCases.flatMap(({name,value})=>[['tx'],['tx','opts'],['signature']].flatMap(path=>[123,124,125,126,127,140].map(depth=>{
 const input=structuredClone(value);let cursor=input;for(const key of path)cursor=cursor[key];
 let extra=17;for(let i=0;i<depth;i++)extra=[extra];cursor.extra=extra;
 const encoded=JSON.stringify(input),parsed=JSON.parse(encoded);cursor=parsed;for(const key of path)cursor=cursor[key];
 let count=0,leaf=cursor.extra;while(Array.isArray(leaf)){count++;leaf=leaf[0];}assert.equal(count,depth);assert.equal(leaf,17);
 return{name,path,depth,encoded};
})));
const signedUnknownStrings=signedCases.flatMap(({name,value})=>[['tx'],['tx','opts'],['signature']].flatMap(path=>['','plain','é','👩','\ufeff','\u0085','\ud800','\udfff','x\ud800y'].map(text=>{
 const input=structuredClone(value);let cursor=input;for(const key of path)cursor=cursor[key];cursor.extra=text;
 const encoded=JSON.stringify(input),parsed=JSON.parse(encoded);cursor=parsed;for(const key of path)cursor=cursor[key];assert.equal(cursor.extra,text);
 return{name,path,encoded,representable:text.isWellFormed()};
})));
const receiptNode=tree.statements.find(n=>ts.isInterfaceDeclaration(n)&&n.name.text==='TxReceipt');
assert.ok(receiptNode&&!receiptNode.heritageClauses?.length,'TxReceipt unsupported interface heritage');
const receiptSchema=receiptNode.members.map(f=>({name:f.name.text,type:f.type.getText(tree),optional:!!f.questionToken}));
const receiptTagType=checker.getTypeFromTypeNode(receiptNode.members.find(f=>f.name.text==='txType').type);
assert.ok(receiptTagType.isUnion(),'TxReceipt.txType closed numeric union');
const receiptTags=receiptTagType.types.map(type=>{assert.ok(type.flags & ts.TypeFlags.NumberLiteral,'TxReceipt.txType numeric literal member');return type.value;}).sort((a,b)=>a-b);
const receiptBase={txHash:'receipt-hash',txType:14,status:7.25,blockNumber:31.5,blockTimestamp:47.75,gasUsed:'gas-used',error:'receipt-error',data:{left:11,right:23,nested:[{label:'payload-label',other:'payload-other'}]}};
const receiptNumbers=receiptSchema.filter(f=>f.type==='number').flatMap(f=>['-0','-0.0','-0e0','0','1e3','1e400','-1e400','-1e-4000','16777217.25','9007199254740993'].map(raw=>{
 const encoded=JSON.stringify({...receiptBase,[f.name]:'__NUMBER__'}).replace('"__NUMBER__"',raw);
 return{field:f.name,raw,encoded,bits:bits(JSON.parse(encoded)[f.name])};
}));
const receiptPayloadNumbers=['-0','-0.0','-0e0','0','1e3','1e400','-1e400','-1e-4000','16777217.25','9007199254740993'].flatMap(raw=>['root','nested'].map(location=>{
 const encoded=JSON.stringify({...receiptBase,data:'__DATA__'}).replace('"__DATA__"',location==='root'?raw:'{"value":'+raw+',"other":13}');
 const parsed=JSON.parse(encoded).data;return{location,raw,encoded,bits:bits(location==='root'?parsed:parsed.value)};
}));
const receiptNonJson=[undefined,17n,()=>1,Symbol('receipt')].map(data=>{
 const receipt={...receiptBase,data};assert.ok(Object.hasOwn(receipt,'data'));
 return{kind:typeof receipt.data,own:true};
});
const receiptStrings=['txHash','gasUsed','error','data','extra'].flatMap(field=>['','plain','é','👩','\ufeff','\u0085','\ud800','\udfff','x\ud800y'].map(text=>{
 const encoded=JSON.stringify({...receiptBase,[field]:text});assert.equal(JSON.parse(encoded)[field],text);
 return{field,units:Array.from({length:text.length},(_,i)=>text.charCodeAt(i)),encoded,representable:text.isWellFormed()};
}));
const receiptDepth=[0,2,125,126,127,140].map(depth=>{
 let data=17;for(let i=0;i<depth;i++)data=[data];const encoded=JSON.stringify({...receiptBase,data});
 let leaf=JSON.parse(encoded).data,count=0;while(Array.isArray(leaf)){count++;leaf=leaf[0];}assert.equal(count,depth);assert.equal(leaf,17);
 return{depth,encoded};
});
// Preserve checker-derived literal order before forcing all program diagnostics.
const diagnostics=ts.getPreEmitDiagnostics(program);
assert.equal(diagnostics.length,0,`transaction source program diagnostics must be empty: ${diagnostics.length}\n${ts.formatDiagnostics(diagnostics,{getCanonicalFileName:file=>file,getCurrentDirectory:()=>path.dirname(file),getNewLine:()=> '\n'})}`);
process.stdout.write(JSON.stringify({receiptTags,receiptNonJson,receiptSchema,receiptBase,receiptNumbers,receiptPayloadNumbers,receiptStrings,receiptDepth,schemas,complete,numeric,strings,tags,nestedNumbers,nestedStrings,variants,signedSchema,signedCases,signedNumeric,signedStrings,signedDepth,signedUnknownStrings}));
