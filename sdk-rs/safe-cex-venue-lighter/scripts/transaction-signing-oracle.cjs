'use strict';
// Actual source composition with deterministic crypto callbacks, not arithmetic proof.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
const source=fs.readFileSync(path.join(root,'packages/lighter-sdk/src/client/transaction-client.ts'),'utf8');
const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});assert.equal(compiled.diagnostics.length,0,'signing source transpile diagnostics');
const units=value=>Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
function encode(v){if(v===undefined)return{undefined:true};if(typeof v==='bigint')return{bigint:v.toString()};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{number:b.toString('hex')}}if(typeof v==='string')return{utf16:units(v)};if(Array.isArray(v))return v.map(encode);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)]));return v;}
const file=path.join(root,'packages/lighter-sdk/src/client/http-types.ts'),tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);assert.equal(tree.parseDiagnostics.length,0);
function shape(n){if(ts.isTypeLiteralNode(n))return fields(n.members);if(ts.isArrayTypeNode(n))return{array:shape(n.elementType)};if(ts.isUnionTypeNode(n))return{union:n.types.map(shape)};if(ts.isLiteralTypeNode(n))return{literal:ts.isNumericLiteral(n.literal)?Number(n.literal.text):n.literal.text};return n.getText(tree);}
function fields(members){return members.map(f=>{assert.ok(ts.isPropertySignature(f),'HTTP supported property');return[f.name.getText(tree),shape(f.type),!!f.questionToken]});}
const required=(name,type)=>[name,type,false],optional=(name,type)=>[name,type,true];
const lit=value=>({literal:value}),union=(...values)=>({union:values.map(lit)});
const groupFields=[required('orderId','bigint'),required('marketId','number'),required('isBuy','boolean'),required('price','bigint'),required('amount','bigint'),optional('reduceOnly','boolean'),optional('orderTypeValue','number'),optional('timeInForceValue','number'),optional('triggerPrice','bigint'),optional('orderExpiry','bigint')];
const createFields=[...groupFields.slice(0,5),optional('postOnly','boolean'),...groupFields.slice(5),optional('integratorAccountIndex','bigint'),optional('integratorMakerFee','number'),optional('integratorTakerFee','number')];
const kinds=['create_order','cancel_order','transfer','withdrawal','update_leverage','approve_integrator','create_grouped_orders'];
const expected={TxInfo:[required('type',union(...kinds)),required('data','string'),required('signature','string'),required('accountIndex','bigint'),required('apiKeyIndex','number'),required('nonce','bigint'),optional('expiredAt','bigint')],LighterGroupedOrderDetails:groupFields,
 L2CreateOrderTxInfo:[required('type',lit('create_order')),required('orderDetails',createFields)],
 L2CreateGroupedOrdersTxInfo:[required('type',lit('create_grouped_orders')),required('groupedOrderDetails',[required('groupingType',union(1,2,3)),required('orders',{array:'LighterGroupedOrderDetails'}),optional('integratorAccountIndex','bigint'),optional('integratorMakerFee','number'),optional('integratorTakerFee','number')])],
 L2CancelOrderTxInfo:[required('type',lit('cancel_order')),required('cancelDetails',[required('orderId','bigint'),required('marketId','number'),optional('expiredAt','bigint')])],
 L2TransferTxInfo:[required('type',lit('transfer')),required('transferDetails',[required('toAccountIndex','bigint'),required('amount','bigint'),required('tokenId','number')])],
 L2UpdateLeverageTxInfo:[required('type',lit('update_leverage')),required('leverageDetails',[required('marketId','number'),required('initialMarginFraction','number'),required('marginMode',union(0,1)),optional('expiredAt','bigint')])],
 L2ApproveIntegratorTxInfo:[required('type',lit('approve_integrator')),required('approveIntegratorDetails',[required('integratorAccountIndex','bigint'),required('maxPerpsTakerFee','number'),required('maxPerpsMakerFee','number'),required('maxSpotTakerFee','number'),required('maxSpotMakerFee','number'),required('approvalExpiry','bigint'),optional('expiredAt','bigint'),optional('l1Signature','string')])],
 L2WithdrawalTxInfo:[required('type',lit('withdrawal'))]};
for(const [name,schema]of Object.entries(expected)){const declarations=tree.statements.filter(n=>ts.isInterfaceDeclaration(n)&&n.name.text===name);assert.equal(declarations.length,1,`${name} inventory`);const d=declarations[0];assert.deepEqual(d.heritageClauses?.flatMap(c=>c.types.map(t=>t.getText(tree)))||[],name.startsWith('L2')?['TxInfo']:[],`${name} heritage`);assert.deepEqual(fields(d.members),schema,`${name} source fields`);}
const aliases=tree.statements.filter(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='AnyTxInfo');assert.equal(aliases.length,1);assert.deepEqual(shape(aliases[0].type),{union:['L2CreateOrderTxInfo','L2CreateGroupedOrdersTxInfo','L2CancelOrderTxInfo','L2TransferTxInfo','L2UpdateLeverageTxInfo','L2ApproveIntegratorTxInfo','L2WithdrawalTxInfo']},'AnyTxInfo alternatives');
const detail=(i,full)=>({orderId:(1n<<200n)+BigInt(i),marketId:17.25+i,isBuy:!!(i&1),price:-(1n<<201n)+BigInt(i),amount:(1n<<202n)+BigInt(i),...(full?{reduceOnly:!!(i&2),orderTypeValue:-0,timeInForceValue:Infinity,triggerPrice:(1n<<203n)+BigInt(i),orderExpiry:-(1n<<204n)+BigInt(i)}:{})});
const typeRecords=[];
for(const full of [false,true]){
 const header={data:'data-A\ud800',signature:'signature-B👩',accountIndex:1n<<200n,apiKeyIndex:-0,nonce:-(1n<<201n),...(full?{expiredAt:1n<<202n}:{})};
 for(const kind of kinds)typeRecords.push({record:'TxInfo',input:encode({type:kind,...header})});
 typeRecords.push({record:'LighterGroupedOrderDetails',input:encode(detail(3,full))});
 for(const [record,kind,extra]of[
 ['L2CreateOrderTxInfo','create_order',{orderDetails:{...detail(3,full),...(full?{postOnly:true,integratorAccountIndex:1n<<205n,integratorMakerFee:-0,integratorTakerFee:NaN}:{})}}],
 ['L2CreateGroupedOrdersTxInfo','create_grouped_orders',{groupedOrderDetails:{groupingType:full?3:1,orders:full?[detail(3,true),detail(1,true),detail(3,true),detail(2,true)]:[],...(full?{integratorAccountIndex:1n<<205n,integratorMakerFee:Infinity,integratorTakerFee:-0}:{})}}],
 ['L2CancelOrderTxInfo','cancel_order',{cancelDetails:{orderId:1n<<203n,marketId:NaN,...(full?{expiredAt:1n<<204n}:{})}}],
 ['L2TransferTxInfo','transfer',{transferDetails:{toAccountIndex:1n<<203n,amount:-(1n<<204n),tokenId:Infinity}}],
 ['L2UpdateLeverageTxInfo','update_leverage',{leverageDetails:{marketId:-0,initialMarginFraction:NaN,marginMode:-0,...(full?{expiredAt:1n<<204n}:{})}}],
 ['L2ApproveIntegratorTxInfo','approve_integrator',{approveIntegratorDetails:{integratorAccountIndex:1n<<203n,maxPerpsTakerFee:251,maxPerpsMakerFee:17,maxSpotTakerFee:43,maxSpotMakerFee:89,approvalExpiry:1n<<204n,...(full?{expiredAt:1n<<205n,l1Signature:'L1\ud800👩'}:{})}}],
 ['L2WithdrawalTxInfo','withdrawal',{}]]){
  const input=encode({type:kind,...header,...extra});typeRecords.push({record,input},{record:'AnyTxInfo',input});
 }
}
const cases=[],config={chainId:304,accountIndex:9007199254740993n,apiKeyIndex:13},privateKey='key\ud800-A',order={orderId:301n,marketId:17,isBuy:true,price:2401n,amount:2413n};
async function capture(method,args,mode={}){
 let clockCalls=0;const trace=[],counts={};let throwToken;
 if(mode.kind==='thrown'){throwToken=mode.throwType==='string'?'thrown\ud800':mode.throwType==='null'?null:mode.throwType==='symbol'?Symbol('opaque'):Object.assign(new Error('thrown\ud800'),{name:'名前\ud800'});}
 const log=(method,args=[],receiver='bound')=>{trace.push({method,args:encode(args),receiver});const count=counts[method]=(counts[method]||0)+1;if(mode.method===method&&count===(mode.occurrence||1)){if(mode.kind==='thrown')throw throwToken;if(mode.kind==='returned')return{error:'failure\ud800👩'};if(mode.kind==='false')return false;}return undefined};
 const crypto={isReady(){const result=log('ready');return result===undefined?true:result},poseidon2Hash(input){const result=log('hash',[input]);return result===undefined?`hash-${counts.hash}\ud800`:result},poseidon2HashTwoToOne(left,right){const result=log('aggregate',[left,right]);return result===undefined?`aggregate-${counts.aggregate}\ud800`:result},schnorrSign(input,key){const result=log('sign',[input,key]);return result===undefined?'signature\udfff':result},get poseidon2HashFp5(){log('getFp5');if(mode.missingFp5)return undefined;return function(input){const receiver=this===undefined?'unbound':'bound';const result=log('fp5',[input],receiver);return result===undefined?(counts.fp5===1?'0xbase\ud800':counts.fp5===2?'0Xattrs\udfff':'fp5-final👩'):result}}};
 const context={exports:{},TextEncoder:mode.encoder==='unavailable'||mode.encoder==='buffer'?undefined:TextEncoder,Buffer:mode.encoder==='unavailable'?undefined:Buffer,Date:{now:()=>1700000000000+7*clockCalls++}};
 vm.runInNewContext(compiled.outputText,context);const client=new context.exports.TransactionClient({...config,httpClient:{},crypto,privateKey});let output;
 try{output={ok:encode(await client[method](...args))}}catch(e){output=Object.is(e,throwToken)&&mode.kind==='thrown'?{thrown:{sameIdentity:true,type:mode.throwType||'error'}}:{error:{name:e.name,units:units(e.message)}};}
 cases.push({method,args:encode(args),mode,output,trace,clockCalls});
}
(async()=>{
 for(const method of ['signTransaction','signCreateOrderTransaction']){
  const args=method==='signTransaction'?['data\ud800-A']:['data\ud800-A','attrs\udfff-B'];
  await capture(method,args);await capture(method,args,{method:'ready',kind:'false'});
  const operations=method==='signTransaction'?[['hash',1],['sign',1]]:[['fp5',1],['fp5',2],['fp5',3],['sign',1]];
  for(const [op,occurrence]of operations)for(const kind of ['returned','thrown'])await capture(method,args,{method:op,occurrence,kind,throwType:'error'});
 }
 for(const attributes of [undefined,'',' '])await capture('signCreateOrderTransaction',['data\ud800-A',attributes]);
 await capture('signCreateOrderTransaction',['data-A','attrs-B'],{missingFp5:true});
 for(const throwType of ['string','null','symbol'])await capture('signTransaction',['data-A'],{method:'hash',kind:'thrown',throwType});
 await capture('signCreateOrderTransaction',['data-A','attrs-B'],{method:'getFp5',kind:'thrown',throwType:'error'});
 await capture('signTransaction',['data-A'],{method:'ready',kind:'thrown',throwType:'error'});
 for(const full of [false,true])await capture('buildCreateOrderTransaction',[{...order,...(full?{integratorAccountIndex:501n,integratorMakerFee:17,integratorTakerFee:43,postOnly:true,reduceOnly:false}:{})},7n]);
 const parent={...order,orderId:101n},stop={...order,orderId:102n,amount:0n,isBuy:false,orderType:'stop_loss',reduceOnly:true},take={...stop,orderId:103n,orderType:'take_profit'};
 const groups=[{groupingType:1,orders:[parent,stop]},{groupingType:2,orders:[stop,take]},{groupingType:3,orders:[parent,stop,take]}];
 for(const group of groups)for(const full of [false,true])await capture('buildCreateGroupedOrdersData',[{...group,...(full?{integratorAccountIndex:501n,integratorMakerFee:17,integratorTakerFee:43}:{})},7n]);
 for(const [method,occurrence]of [['hash',1],['hash',2],['hash',3],['aggregate',1],['aggregate',2]])for(const kind of ['returned','thrown'])await capture('buildCreateGroupedOrdersData',[groups[2],7n],{method,occurrence,kind,throwType:'error'});
 await capture('buildCreateGroupedOrdersData',[groups[0],7n],{method:'ready',kind:'false'});
 await capture('buildCreateGroupedOrdersData',[{groupingType:1,orders:[]},7n],{method:'ready',kind:'false'});
 await capture('buildCreateGroupedOrdersData',[{...groups[0],orders:[parent,{...stop,price:-1n}]},7n]);
 for(const encoder of ['text','buffer','unavailable'])await capture('getAuthToken',[3601n],{encoder});
 await capture('getAuthToken',[-(1n<<200n)],{encoder:'text'});
 await capture('getAuthToken',[3601n],{method:'ready',occurrence:2,kind:'false'});
 await capture('getAuthToken',[3601n],{method:'ready',occurrence:1,kind:'false',encoder:'unavailable'});
 const body={typeRecords,cases,config:encode(config),privateKey:encode(privateKey)};
 const contentHash=require('node:crypto').createHash('sha256').update(JSON.stringify(body)).digest('hex');
 process.stdout.write(JSON.stringify({contentHash,...body}));
})().catch(e=>{process.stderr.write(String(e.stack||e));process.exitCode=1});
