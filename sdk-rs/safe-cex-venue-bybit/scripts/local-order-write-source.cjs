const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=process.cwd(),modules=new Map();
function load(file){
 if(modules.has(file))return modules.get(file);
 const text=fs.readFileSync(file,'utf8'),source=ts.createSourceFile(file,text,99,true);
 assert.equal(source.parseDiagnostics.length,0,file);
 const emitted=ts.transpileModule(text,{compilerOptions:{module:1,target:9},reportDiagnostics:true});
 assert.equal(emitted.diagnostics.length,0,file);
 const exports={};modules.set(file,exports);
 vm.runInNewContext(emitted.outputText,{exports,require(spec){
  assert.ok(spec.startsWith('.')||spec.startsWith('@tealstreet/'),spec);
  const target=spec.startsWith('.')?path.resolve(path.dirname(file),spec):path.join(root,'packages',spec.slice('@tealstreet/'.length));
  return load(fs.existsSync(target+'.ts')?target+'.ts':path.join(target,'index.ts'));
 }},{filename:file});return exports;
}
const enums=load(path.join(root,'packages/safe-cex/src/types.ts'));
const inverseModule=load(path.join(root,'packages/safe-cex/src/exchanges/bybit/bybit.inverse.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),99,true);
assert.equal(source.parseDiagnostics.length,0);
const klass=source.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BybitExchange');
const names=['recordRecentlyPlacedOrderFromOpts','applyLocalOrderUpdateAfterRestWrite','remainingInStoreUnits'];
const fields=names.map(name=>{
 const found=klass.members.filter(n=>n.name?.getText(source)===name);
 assert.equal(found.length,1,name);return found[0].getText(source);
});
const program=ts.transpileModule('class Subject {'+fields.join('\n')+'}\nexports.Subject=Subject;',{
 compilerOptions:{target:9,module:1},reportDiagnostics:true});
assert.equal(program.diagnostics.length,0);
function observe(value){
 if(value===undefined)return {undefined:true};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {number:Number.isNaN(value)?'nan':b.toString('hex')};}
 if(typeof value==='string')return {stringUnits:Array.from({length:value.length},(_,i)=>value.charCodeAt(i))};
 if(Array.isArray(value))return Array.from(value,observe);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
 return value;
}
function make({parseReceiverWitness=false}={}){
 const events=[],clock={getCurrentTimeMs(){assert.equal(this,clock,'source clock receiver');events.push('clock');return 1234.5;}};
 const context=vm.createContext({exports:{},...enums,isBybitInverseSymbol:inverseModule.isBybitInverseSymbol,virtualClock:clock});
 const parseReceiverCalls=vm.runInContext('[]',context);
 if(parseReceiverWitness){
  context.parseReceiverCalls=parseReceiverCalls;
  vm.runInContext("globalThis.parseFloat=((original)=>function(value){'use strict';parseReceiverCalls.push({receiverUndefined:this===undefined,value});return Reflect.apply(original,undefined,[value]);})(parseFloat);",context);
 }
 vm.runInContext(program.outputText,context);
 const owner=new context.exports.Subject();
 owner.recordRecentlyPlacedOrder=function(order){assert.equal(this,owner,'source recording receiver');events.push(['record',observe(order)]);};
 owner.shouldUseLocalRestOrderLifecycleFallback=function(){assert.equal(this,owner,'source fallback receiver');return true;};
 owner.recentlyPlacedOrders=new Map();
 owner.store={addOrUpdateOrders(orders){assert.equal(this,owner.store,'source publication receiver');events.push(['add',observe(orders)]);}};
 return {owner,events,parseReceiverCalls};
}
const base={symbol:'BTCUSDT',type:enums.OrderType.Limit,side:enums.OrderSide.Buy,amount:7,price:11,triggerPrice:13,reduceOnly:true};
const cases=[];
const quantities=[undefined,'0','-0','NaN','Infinity','-Infinity','0x10','1.75tail',null,[],['2.5'],{}, {toString:null},'\ud800'];
const prices=[undefined,'0','4.25tail',null];
for(const qty of quantities)for(const price of prices){
 const x=make(),payload={qty,price,triggerPrice:'17.5'};
 try{x.owner.recordRecentlyPlacedOrderFromOpts('order-1',{...base},payload);cases.push({input:observe(payload),events:x.events});}
 catch(error){cases.push({input:observe(payload),events:x.events,error:{name:error.name,message:error.message}});}
}
assert.equal(cases.length,56,'fixed quantity14 x price4 declaration');
for(const record of cases){
 if(record.error){assert.equal(record.events.length,0,'coercion failure precedes clock and record');}
 else{assert.equal(record.events[0],'clock');assert.equal(record.events[1][0],'record');assert.equal(record.events.length,2);}
}
const errors=cases.filter(c=>c.error);
assert.equal(errors.length,4,'own null toString rejects all four price profiles');
const surrogate=cases.slice(-4);assert.ok(surrogate.every(c=>!c.error),'source accepts lone-unit scalar qty and falls back');
const supplements=[];
const profiles=[
 ['conditional-clear',{type:enums.OrderType.Conditional}, {qty:'3',price:'2'}],
 ['market-retained',{type:enums.OrderType.Market}, {qty:'3',price:'2'}],
 ['explicit-reduce-false',{reduceOnly:false}, {qty:'3',price:'2'}],
 ['explicit-tif',{timeInForce:enums.OrderTimeInForce.ImmediateOrCancel}, {qty:'3',price:'2'}],
 ['zero-amount',{amount:0}, {}],
 ['negative-zero-amount',{amount:-0}, {}],
 ['nan-amount',{amount:NaN}, {}],
 ['zero-price',{price:0}, {price:'0'}],
 ['nan-price',{price:NaN}, {price:'NaN'}],
 ['infinite-price',{price:Infinity}, {price:'0'}],
];
for(const [kind,changes,payload] of profiles){
 const x=make();x.owner.recordRecentlyPlacedOrderFromOpts('order-1',{...base,...changes},payload);
 assert.equal(x.events.length,2);assert.equal(x.events[0],'clock');assert.equal(x.events[1][0],'record');
 const actual=x.events[1][1];
 if(kind==='conditional-clear'||kind==='explicit-reduce-false')assert.equal(actual.reduceOnly,false,kind);
 if(kind==='market-retained')assert.equal(actual.reduceOnly,true,kind);
 if(['zero-amount','negative-zero-amount','nan-amount'].includes(kind))assert.deepEqual(actual.amount,{number:'0000000000000000'},kind);
 supplements.push({kind,opts:observe({...base,...changes}),payload:observe(payload),events:x.events});
}
assert.equal(supplements.length,10,'fixed option branch declarations');
const updates=[];
const updateProfiles=[
 ['ordinary',{},false,true],['recent',{},true,true],['fallback-off',{},true,false],
 ['amount',{amount:10},true,true],['remaining-override',{amount:10,remaining:27},true,true],
 ['zero-amount',{amount:0},true,true],['nan-amount',{amount:NaN},true,true],
 ['infinite-amount',{amount:Infinity},true,true],['id-change',{id:'replacement'},true,true],
 ['filled-change',{amount:10,filled:8},true,true],
];
for(const [kind,patch,recent,fallback] of updateProfiles){
 const x=make(),info={child:'retained'};
 const order={id:'order-1',status:enums.OrderStatus.Open,symbol:'BTCUSDT',type:enums.OrderType.Limit,side:enums.OrderSide.Buy,price:11,amount:7,filled:2,remaining:5,reduceOnly:true,timeInForce:enums.OrderTimeInForce.GoodTillCancel,lastUpdate:42,info};
 const prior={placedAt:12,order,extra:info};if(recent)x.owner.recentlyPlacedOrders.set(order.id,prior);
 x.owner.shouldUseLocalRestOrderLifecycleFallback=function(){assert.equal(this,x.owner,'source update fallback receiver');x.events.push('fallback');return fallback;};
 let passed;
 x.owner.store.addOrUpdateOrders=function(orders){assert.equal(this,x.owner.store,'source update publication receiver');passed=orders;x.events.push(['add',observe(orders)]);};
 x.owner.applyLocalOrderUpdateAfterRestWrite(order,patch);
 if(!fallback){assert.deepEqual(x.events,['fallback']);assert.equal(passed,undefined);}
 else{
  assert.equal(x.events.length,3);assert.equal(x.events[0],'fallback');assert.equal(x.events[1],'clock');
  assert.equal(passed.length,1);assert.notEqual(passed[0],order);assert.equal(passed[0].info,info);
  assert.equal(passed[0].lastUpdate,1234.5);
  if(typeof patch.amount==='number')assert.ok(Object.is(passed[0].remaining,Math.max(0,patch.amount-2)),kind+' remaining uses original filled');
  if(recent){const next=x.owner.recentlyPlacedOrders.get('order-1');assert.notEqual(next,prior);assert.equal(next.order,passed[0]);assert.equal(next.extra,info);assert.equal(prior.order,order);}
 }
 assert.equal(order.lastUpdate,42);assert.equal(order.filled,2);
 updates.push({kind,patch:observe(patch),events:x.events,recent:[...x.owner.recentlyPlacedOrders].map(([key,v])=>[key,observe(v)]),passed:observe(passed)});
}
assert.equal(updates.length,10,'fixed local update declarations');
const dependencyProfiles=[];
{
 const kind='strict-global-parse-float',x=make({parseReceiverWitness:true}),payload={qty:'3.5',price:'4.25',triggerPrice:'99'};
 x.owner.recordRecentlyPlacedOrderFromOpts('order-1',{...base},payload);
 assert.equal(x.parseReceiverCalls.length,2,'source strict parseFloat fixed call floor');
 assert.ok(x.parseReceiverCalls.every(call=>call.receiverUndefined===true),'source strict bare parseFloat receiver undefined');
 assert.equal(x.parseReceiverCalls[0].value,payload.qty);assert.equal(x.parseReceiverCalls[1].value,payload.price);
 assert.equal(x.events.length,2);assert.equal(x.events[0],'clock');assert.equal(x.events[1][0],'record');
 assert.deepEqual(x.events[1][1].amount,observe(3.5));assert.deepEqual(x.events[1][1].price,observe(4.25));
 dependencyProfiles.push({kind,payload:observe(payload),calls:Array.from(x.parseReceiverCalls,call=>({receiverUndefined:call.receiverUndefined,input:observe(call.value)})),events:x.events});
}
const inverseRecords=[];
for(const [kind,changes,payload] of [
 ['contracts-at-price',{},{qty:'100',price:'25000'}],
 ['fallback-price',{},{qty:'100',price:'0'}],
 ['non-positive-price',{price:-5},{qty:'100',price:'-5'}],
 ['dated-future',{symbol:'ETHUSDZ26'},{qty:'30',price:'3'}],
]){
 const x=make();x.owner.recordRecentlyPlacedOrderFromOpts('order-1',{...base,symbol:'BTCUSD',...changes},payload);
 assert.equal(x.events.length,2);assert.equal(x.events[1][0],'record');
 inverseRecords.push({kind,opts:observe({...base,symbol:'BTCUSD',...changes}),payload:observe(payload),events:x.events});
}
assert.deepEqual(inverseRecords[0].events[1][1].remaining,observe(0.004),'inverse remaining in coin');
assert.deepEqual(inverseRecords[2].events[1][1].remaining,observe(100),'non-positive price keeps contracts');
const inverseUpdates=[];
for(const [kind,patch,extra] of [
 ['price-amend',{price:20},undefined],['amount-amend',{amount:10},undefined],
 ['retained-native',{},{remainingNative:50,keep:'x'}],['zero-price',{price:0},undefined],
 ['amount-and-price',{amount:9,price:4},{remainingNative:50,keep:'x'}],
]){
 const x=make(),order={id:'order-1',symbol:'BTCUSD',price:11,amount:7,filled:2,remaining:5,lastUpdate:42};
 if(extra)order.extra=extra;
 x.owner.shouldUseLocalRestOrderLifecycleFallback=function(){x.events.push('fallback');return true;};
 let passed;x.owner.store.addOrUpdateOrders=function(orders){passed=orders;x.events.push(['add',observe(orders)]);};
 x.owner.applyLocalOrderUpdateAfterRestWrite(order,patch);
 if(extra)assert.equal(order.extra,extra,'original extra untouched');
 inverseUpdates.push({kind,order:observe(order),patch:observe(patch),events:x.events,passed:observe(passed)});
}
assert.deepEqual(inverseUpdates[0].passed[0].remaining,observe(0.25),'price-only amend rescales coin remaining');
assert.equal(inverseRecords.length+inverseUpdates.length,9,'inverse profile floor');
assert.equal(cases.length+supplements.length+updates.length,76,'retained baseline profile floor');
assert.equal(dependencyProfiles.length,1,'separate dependency receiver profile floor');
console.log(JSON.stringify({scope:'source-only extracted maintained methods; no native implementation, controls or annotation claim',declaredCases:56,cases,supplements,updates,declaredDependencyCases:1,dependencyProfiles,inverseRecords,inverseUpdates}));
