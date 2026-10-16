const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {write,privateApi,ts,writeCode}=require('./load-starbase.cjs');
const source=ts.createSourceFile('orders.ts',writeCode);
const assessor=source.statements.filter(ts.isVariableStatement).flatMap(n=>Array.from(n.declarationList.declarations)).find(d=>d.name.getText(source)==='assessCoinbaseStarbaseWriteResultParity').initializer;
assert(ts.isTypeReferenceNode(assessor.type));
const assessmentReturn={name:assessor.type.typeName.getText(source),args:assessor.type.typeArguments.map(t=>t.getText(source))};
const {captureHeader,preserveSpecialNumbers,numberBits}=require('./special-numbers.cjs');
const orderBase={instrument_name:'BTC_USDC-PERPETUAL',post_only:false,order_id:'order-1',order_state:'open',order_type:'limit',time_in_force:'good_til_cancelled',direction:'buy',price:17,label:'',creation_timestamp:101,last_update_timestamp:102};
const tradeBase={instrument_name:'BTC_USDC-PERPETUAL',matching_id:null,order_type:'limit',fee_currency:'USDC',trade_id:'trade-1',trade_seq:5,order_id:'order-1',timestamp:101,direction:'buy',state:'filled',price:7,mark_price:11,index_price:13,amount:17,fee:-19};
function capture(fn){try{return preserveSpecialNumbers({ok:fn()});}catch(error){return {error:{name:error.name,message:error.message}};}}
// Input transport retains -0 before JSON can erase it. Missing object fields
// represent source undefined, which these explicit decoders test directly.
function inputJson(value){if(value===undefined)return undefined;if(value===null||typeof value!=='object')return Object.is(value,-0)?'-0':JSON.stringify(value);if(Array.isArray(value))return '['+value.map(v=>inputJson(v)??'null').join(',')+']';return '{'+Object.entries(value).filter(([,v])=>v!==undefined).map(([k,v])=>JSON.stringify(k)+':'+inputJson(v)).join(',')+'}';}
const values=[undefined,null,false,true,0,-0,1,'','a',' ', '\uFEFF','\u0085','💰',[],{},['a'],['a','a'],['a',''],['b','a']];
const cases=[];
function add(name,input){cases.push({name,...input===undefined?{}:{inputJson:inputJson(input)},result:capture(()=>write[name](input))});}
for(const value of values){add('decodeCoinbaseStarbaseWriteResult',value);add('decodeCoinbaseStarbaseCancelResult',value);}
for(const order of [...values,orderBase])for(const trades of [...values,[],[tradeBase]])add('decodeCoinbaseStarbaseWriteResult',{order,trades});
const variants=[orderBase];
for(const field of ['trigger_order_id','oto_order_ids','primary_order_id'])for(const value of values)variants.push({...orderBase,[field]:value});
for(const trigger_order_id of values)for(const primary_order_id of values)variants.push({...orderBase,trigger_order_id,primary_order_id,oto_order_ids:['b','a']});
for(const order of variants){add('decodeCoinbaseStarbaseCancelResult',order);add('decodeCoinbaseStarbaseWriteResult',{order,trades:[]});add('decodeCoinbaseStarbaseWriteResult',{order,trades:[tradeBase]});}
for(const trades of [[],[tradeBase],[tradeBase,{...tradeBase,trade_id:'trade-2',instrument_name:'ETH_USDC-PERPETUAL'}],[tradeBase,tradeBase],[tradeBase,tradeBase,{...tradeBase,fee:null}],[{...tradeBase,trade_id:'trade-2'},tradeBase]])for(const order of [orderBase,{...orderBase,primary_order_id:null},{...orderBase,oto_order_ids:['a','a']},{...orderBase,price:-0,creation_timestamp:-0,last_update_timestamp:-0,amount:-0,filled_amount:-0}])add('decodeCoinbaseStarbaseWriteResult',{order,trades});
const fixturePath=path.resolve(__dirname,'../../../packages/safe-cex/src/__tests__/audit-regressions/coinbase.starbase-orders.fixtures.ts');
const fixtureExports={};
const privateFixturePath=path.resolve(__dirname,'../../../packages/safe-cex/src/__tests__/audit-regressions/coinbase.starbase-private.fixtures.ts');
const privateFixtureExports={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(privateFixturePath,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:privateFixtureExports});
vm.runInNewContext(ts.transpileModule(fs.readFileSync(fixturePath,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:fixtureExports,require:()=>privateFixtureExports});
const sharedOrderFixtureIdentityInput={order:{...fixtureExports.STARBASE_WRITE_ORDER_FIXTURE,post_only:true,reduce_only:false,amount:0.14,filled_amount:0.03,creation_timestamp:1800000000123,last_update_timestamp:1800000000456},trades:[]};
const sharedOrderFixtureIdentitySupplement={label:'field-identity-supplement: starbase-orders fixture',input:sharedOrderFixtureIdentityInput,result:capture(()=>write.decodeCoinbaseStarbaseWriteResult(sharedOrderFixtureIdentityInput))};
const identities=[];const decodedOrder=privateApi.decodeCoinbaseStarbaseOrder({...orderBase,order_id:'canonical',label:'correlation'});
for(const raw of [{},...variants,{order_id:'ignored',label:'ignored'}])identities.push({rawJson:inputJson(raw),result:capture(()=>write.decodeOrderIdentity(raw,decodedOrder))});
const assessments=[];
for(const hasTrade of [false,true])for(const amount of [undefined,-0,0,-1,1,0.5,Number.MIN_VALUE,Number.MAX_VALUE,NaN,Infinity,-Infinity]){
 const result=write.decodeCoinbaseStarbaseWriteResult({order:orderBase,trades:hasTrade?[tradeBase]:[]});if(amount!==undefined)result.order.gatewayFilledAmount=amount;
 const assessed=write.assessCoinbaseStarbaseWriteResultParity(result);assert.strictEqual(assessed.status==='ready'?assessed.value:assessed.intent,result,'gate retains original result identity');
 assessments.push({...captureHeader(),hasTrade,...amount===undefined?{}:{amountBits:numberBits(amount)},result:preserveSpecialNumbers(assessed)});
}
process.stdout.write(JSON.stringify({cases,identities,assessments,orderBase,tradeBase,assessmentReturn,sharedOrderFixtureIdentitySupplement}));
