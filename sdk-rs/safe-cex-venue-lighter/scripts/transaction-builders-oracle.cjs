// Executes actual private TransactionClient builders; no network or cryptography.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file=path.resolve(__dirname,'../../../packages/lighter-sdk/src/client/transaction-client.ts');
let source=fs.readFileSync(file,'utf8');
const mutation=process.env.LIGHTER_TRANSACTION_BUILDERS_MUTATION;
if (mutation === 'validator-order') {
 const old = `      if (order.marketId !== marketId) {
        throw new Error('Grouped orders must all use the same market id');
      }
      const orderId = order.orderId.toString();
      if (orderIds.has(orderId)) {
        throw new Error(\`Grouped orders contain duplicate client order id \${orderId}\`);
      }`;
 const changed = `      const orderId = order.orderId.toString();
      if (orderIds.has(orderId)) {
        throw new Error(\`Grouped orders contain duplicate client order id \${orderId}\`);
      }
      if (order.marketId !== marketId) {
        throw new Error('Grouped orders must all use the same market id');
      }`;
 assert.ok(source.includes(old),'validator-order exact source replacement');
 source=source.replace(old,changed);
}
const tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
assert.equal(tree.parseDiagnostics.length,0,'transaction builder source syntax');
const expected={
 CreateOrderParams:[['orderId','bigint'],['marketId','number'],['isBuy','boolean'],['price','bigint'],['amount','bigint'],['postOnly','boolean',true],['reduceOnly','boolean',true],['orderType',"'limit' | 'market' | 'stop_loss' | 'stop_loss_limit' | 'take_profit' | 'take_profit_limit'",true],['triggerPrice','bigint',true],['orderExpiry','bigint',true],['expiredAt','bigint',true],['timeInForce',"'GoodTillCancel' | 'ImmediateOrCancel' | 'FillOrKill' | 'PostOnly'",true],['integratorAccountIndex','bigint',true],['integratorMakerFee','number',true],['integratorTakerFee','number',true]],
 CreateGroupedOrdersParams:[['groupingType','LighterGroupedOrderType'],['orders','CreateOrderParams[]'],['integratorAccountIndex','bigint',true],['integratorMakerFee','number',true],['integratorTakerFee','number',true]],
 CancelOrderParams:[['orderId','bigint'],['marketId','number']],
 TransferParams:[['toAccountIndex','bigint'],['amount','bigint'],['tokenId','number',true]],
 UpdateLeverageParams:[['marketId','number'],['leverage','number'],['marginMode','0 | 1']],
 ApproveIntegratorParams:[['integratorAccountIndex','bigint'],['maxPerpsTakerFee','number'],['maxPerpsMakerFee','number'],['maxSpotTakerFee','number'],['maxSpotMakerFee','number'],['approvalExpiry','bigint'],['nonce','bigint',true],['l1Signature','string',true]],
};
for(const [name,fields] of Object.entries(expected)) {
 const decl=tree.statements.filter(n=>ts.isInterfaceDeclaration(n)&&n.name.text===name);assert.equal(decl.length,1,`${name} source inventory`);assert.ok(!decl[0].heritageClauses?.length,`${name} unsupported heritage`);
 assert.deepEqual(decl[0].members.map(f=>{assert.ok(ts.isPropertySignature(f),`${name} property kind`);return [f.name.getText(tree),f.type.getText(tree).replace(/\s+/g,' '),!!f.questionToken]}),fields.map(([n,t,o])=>[n,t,!!o]),`${name} source schema`);
}
const grouped=tree.statements.filter(n=>ts.isTypeAliasDeclaration(n)&&n.name.text==='LighterGroupedOrderType');assert.equal(grouped.length,1);assert.equal(grouped[0].type.getText(tree),'1 | 2 | 3','grouped type literal union');
const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});assert.equal(compiled.diagnostics.length,0,'builder transpile diagnostics');
let clockCalls=0;
const context={exports:{},Date:{now:()=>1700000000000+7*clockCalls++}};vm.runInNewContext(compiled.outputText,context,{filename:file});
const Client=context.exports.TransactionClient;
const config={chainId:304,accountIndex:9007199254740993n,apiKeyIndex:13};
const client=new Client({...config,httpClient:{},crypto:{},privateKey:'not-used'});
const bits=value=>{const b=Buffer.alloc(8);b.writeDoubleBE(value);return b.toString('hex')};
const units=value=>Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
function encode(value) {
 if(value===undefined)return {undefined:true};
 if(typeof value==='bigint')return {bigint:value.toString()};
 if(typeof value==='number')return {number:bits(value)};
 if(typeof value==='string')return {utf16:units(value)};
 if(Array.isArray(value))return value.map(encode);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,encode(v)]));
 return value;
}
const order={orderId:301n,marketId:17,isBuy:true,price:2401n,amount:2413n};
const approval={integratorAccountIndex:501n,maxPerpsTakerFee:251,maxPerpsMakerFee:17,maxSpotTakerFee:43,maxSpotMakerFee:89,approvalExpiry:1702592000019n};
const nonce=7n,cases=[];
function add(name,method,params,options,configPatch={}) {
 clockCalls=0;Object.assign(client,config,configPatch);let output;
 try {output={ok:encode(client[method](params,nonce,options))};}catch(error){output={error:{name:error.name,units:units(error.message)}};}
 cases.push({name,method,params:encode(params),nonce:encode(nonce),options:encode(options),config:encode({...config,...configPatch}),output,clockCalls});
}
const types=[undefined,'limit','market','stop_loss','stop_loss_limit','take_profit','take_profit_limit'];
const times=[undefined,'GoodTillCancel','ImmediateOrCancel','FillOrKill','PostOnly'];
for(const orderType of types)for(const timeInForce of times)for(const isBuy of [false,true])for(const postOnly of [false,true])for(const reduceOnly of [false,true]) add(`create-${cases.length}`,'buildCreateOrderData',{...order,orderType,timeInForce,isBuy,postOnly,reduceOnly});
for(const [params,options] of [
 [{...order,expiredAt:7001n},undefined], [{...order,expiredAt:7001n},{expiredAt:7002n,defaultOrderExpiry:8002n}],
 [order,{expiredAt:7002n,defaultOrderExpiry:8002n}], [{...order,orderExpiry:8001n},{expiredAt:7002n,defaultOrderExpiry:8002n}],
 [{...order,orderType:'market'}, {defaultOrderExpiry:8002n}], [{...order,orderType:'stop_loss',timeInForce:'ImmediateOrCancel'}, {defaultOrderExpiry:8002n}],
]) add(`expiry-${cases.length}`,'buildCreateOrderData',params,options);
for(let mask=0;mask<8;mask++)add(`integrator-${mask}`,'buildCreateOrderData',{...order,...(mask&1?{integratorAccountIndex:501n}:{}),...(mask&2?{integratorMakerFee:17}:{}),...(mask&4?{integratorTakerFee:43}:{})});
const invalidCreate=[
 {marketId:NaN},{marketId:Infinity},{marketId:1.5},{marketId:-1},{marketId:4294967296},
 {orderId:1n<<63n},{amount:-(1n<<63n)-1n},{price:-1n},{price:4294967296n},{price:1n<<2000n},{triggerPrice:4294967296n},
 {integratorAccountIndex:-1n,integratorMakerFee:-1,integratorTakerFee:-2},
 {integratorAccountIndex:501n,integratorMakerFee:1.5,integratorTakerFee:-2},
 {integratorAccountIndex:501n,integratorMakerFee:17,integratorTakerFee:Infinity},
 {integratorAccountIndex:1n<<63n,integratorMakerFee:17,integratorTakerFee:43},
 {expiredAt:1n<<63n},{orderExpiry:1n<<63n},
];
for(const patch of invalidCreate)add(`invalid-create-${cases.length}`,'buildCreateOrderData',{...order,...patch});
for(const [index,taker,maker] of [[0n,0,0],[501n,43,17],[0n,43,0],[501n,0,17],[-1n,-2,-3],[1n<<63n,1,2],[1n,1.5,2],[1n,Infinity,2],[1n,2,NaN]]) {
 clockCalls=0;let output;try{output={ok:encode(client.buildCreateOrderL2AttributesData({integratorAccountIndex:index,integratorTakerFee:taker,integratorMakerFee:maker}))}}catch(error){output={error:{name:error.name,units:units(error.message)}}}
 cases.push({name:`attributes-${cases.length}`,method:'buildCreateOrderL2AttributesData',params:encode({integratorAccountIndex:index,integratorTakerFee:taker,integratorMakerFee:maker}),output,clockCalls});
}
for(const orderId of [301n,9007199254740993n,-1n,1n<<63n])add(`cancel-${cases.length}`,'buildCancelOrderData',{orderId,marketId:17});
for(const leverage of [1,3,10000,10000.1,0.5,0,-0,-1,NaN,Infinity,1e-300])for(const marginMode of [0,-0,1])add(`leverage-${cases.length}`,'buildUpdateLeverageData',{marketId:17,leverage,marginMode});
for(const marketId of [NaN,Infinity,1.5,-1,255])add(`leverage-market-${cases.length}`,'buildUpdateLeverageData',{marketId,leverage:3,marginMode:1});
const approvePatches=[{}, {maxPerpsTakerFee:0,maxPerpsMakerFee:0,maxSpotTakerFee:0,maxSpotMakerFee:0,approvalExpiry:0n}, {integratorAccountIndex:-1n},
 ...['maxPerpsTakerFee','maxPerpsMakerFee','maxSpotTakerFee','maxSpotMakerFee'].flatMap(field=>[-1,1.5,1000001,NaN,Infinity].map(value=>({[field]:value}))),
 {approvalExpiry:0n},{approvalExpiry:-1n},{approvalExpiry:1n<<48n},{approvalExpiry:(1n<<48n)-1n},
 {integratorAccountIndex:1n<<63n},{maxPerpsTakerFee:0,maxPerpsMakerFee:0,maxSpotTakerFee:0,maxSpotMakerFee:0},
];
for(const patch of approvePatches)add(`approve-${cases.length}`,'buildApproveIntegratorData',{...approval,...patch});
const parent={...order,orderId:101n},stop={...order,orderId:102n,amount:0n,isBuy:false,orderType:'stop_loss',reduceOnly:true},take={...stop,orderId:103n,orderType:'take_profit'};
const groups=[{groupingType:1,orders:[parent,stop]},{groupingType:2,orders:[stop,take]},{groupingType:3,orders:[parent,stop,take]}];
const validations=[];
function validate(params){let output;try{client.validateCreateGroupedOrdersParams(params);output={ok:true}}catch(error){output={error:{name:error.name,units:units(error.message)}}}validations.push({params:encode(params),output});}
for(const base of groups) {
 validate(base);for(const count of [0,1,2,3,4])validate({...base,orders:Array.from({length:count},(_,i)=>({...parent,orderId:BigInt(i+1)}))});
 for(const [field,value] of [['marketId',-1],['marketId',1.5],['marketId',255],['marketId',NaN],['marketId',Infinity]])validate({...base,orders:base.orders.map((v,i)=>i? v:{...v,[field]:value})});
 validate({...base,orders:base.orders.map((v,i)=>i===1?{...v,marketId:18}:v)});
 validate({...base,orders:base.orders.map((v,i)=>i===1?{...v,orderId:base.orders[0].orderId}:v)});
 for(const i of base.orders.keys())for(const patch of [{amount:1n},{isBuy:!base.orders[i].isBuy},{reduceOnly:false},{orderType:'limit'},{orderType:'stop_loss'},{orderType:'take_profit'}])validate({...base,orders:base.orders.map((v,j)=>i===j?{...v,...patch}:v)});
}
// Composite faults pin source same-market precedence over duplicate-ID checking.
for (const base of groups) validate({...base,orders:base.orders.map((order,index)=>index===1?{...order,marketId:18,orderId:base.orders[0].orderId}:order)});
const transferNumbers=[-0,0,1,304,-1,0.1,0.10000000000000002,Math.PI,4294967296.5,Number.MIN_VALUE,1e-300,Number.MAX_VALUE,NaN,Infinity,-Infinity];
for(const value of transferNumbers)add(`transfer-${cases.length}`,'buildTransferData',{toAccountIndex:701n,amount:709n,tokenId:value},undefined,{chainId:value});
for(const value of [-(1n<<260n),-1n,0n,(1n<<260n)+1n])add(`transfer-bigint-${cases.length}`,'buildTransferData',{toAccountIndex:value,amount:value},undefined,{accountIndex:value});
const infoParams={...order,isBuy:false,reduceOnly:true};
const infoPayload={orderTypeValue:3,timeInForceValue:2,triggerPrice:2399n,orderExpiry:1702419200067n};
const info=encode({params:infoParams,payload:infoPayload,result:client.buildGroupedOrderInfoData(infoParams,infoPayload)});
const recordInputs={CreateOrderParams:[order,{...order,postOnly:true,reduceOnly:false,orderType:'take_profit_limit',triggerPrice:1n<<200n,orderExpiry:-(1n<<200n),expiredAt:1n<<201n,timeInForce:'FillOrKill',integratorAccountIndex:1n<<202n,integratorMakerFee:-0,integratorTakerFee:Infinity}],
 CreateGroupedOrdersParams:[groups[0],{...groups[2],integratorAccountIndex:501n,integratorMakerFee:-0,integratorTakerFee:43}],
 CancelOrderParams:[{orderId:1n<<200n,marketId:-0}],TransferParams:[{toAccountIndex:-(1n<<200n),amount:1n<<201n},{toAccountIndex:701n,amount:709n,tokenId:NaN}],
 UpdateLeverageParams:[{marketId:-0,leverage:Infinity,marginMode:-0}],ApproveIntegratorParams:[approval,{...approval,nonce:1n<<200n,l1Signature:'x\ud800👩'}]};
const getters=[config,{chainId:-0,accountIndex:-(1n<<200n),apiKeyIndex:-0},{chainId:NaN,accountIndex:1n<<200n,apiKeyIndex:Infinity},{chainId:13.5,accountIndex:101n,apiKeyIndex:304.25}].map(config=>{
 Object.assign(client,config);return {config:encode(config),accountIndex:encode(client.getAccountIndex()),apiKeyIndex:encode(client.getApiKeyIndex()),chainId:encode(client.getChainId())};
});
const body={recordInputs:encode(recordInputs),cases,validations,info,getters};
const contentHash=require('node:crypto').createHash('sha256').update(JSON.stringify(body)).digest('hex');
process.stdout.write(JSON.stringify({contentHash,...body}));
