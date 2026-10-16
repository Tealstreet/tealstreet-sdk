const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..'),modules=new Map();
function load(file){
 if(modules.has(file))return modules.get(file);
 const exports={};modules.set(file,exports);
 vm.runInNewContext(ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require(spec){
  assert(spec.startsWith('.')||spec.startsWith('@tealstreet/'),spec);
  const target=spec.startsWith('.')?path.resolve(path.dirname(file),spec):path.join(root,'packages',spec.slice('@tealstreet/'.length));
  return load(fs.existsSync(target+'.ts')?target+'.ts':path.join(target,'index.ts'));
 }},{filename:file});return exports;
}
const enums=load(path.join(root,'packages/safe-cex/src/types.ts'));
const site=load(path.join(root,'packages/safe-cex/src/exchanges/bybit/bybit.site.ts'));
function observe(value){
 if(value===undefined)return {undefined:true};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {number:Number.isNaN(value)?'nan':b.toString('hex')};}
 if(Array.isArray(value))return Array.from(value,observe);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
 return value;
}
const order=(id,type='limit',price=23.5)=>({id,status:'open',symbol:'BTCUSD',type,side:'buy',price,amount:7.25,filled:1.5,remaining:5.75,reduceOnly:false,timeInForce:'GoodTillCancel',lastUpdate:123.5});
const registry=()=>({A:order('a'),B:order('b','conditional'),ReplacementA:order('a','limit',41.25),StoreA:order('a','limit',53.75),SecondStoreA:order('a','limit',67.125),Market:order('m','market')});
function subject(host){const file=path.join(root,'packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts'),source=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true),cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BybitExchange');assert(cls);const names=['recentlyPlacedOrders','recentlyPlacedOrderReconciliationTimers','retiredOrderSymbols','shouldUseLocalRestOrderLifecycleFallback','markOrderSeen','clearRecentlyPlacedOrderReconciliationTimers','shouldReconcileLocalReduceOnlyLimitOrder','scheduleLocalReduceOnlyLimitOrderReconciliation','getFilledOrderReconciliationTarget','recordRecentlyPlacedOrder','retireCancelledOrders','resolveOrderSymbolForFills','preserveRecentlyPlacedOrders'];const body=names.map(n=>{const hits=cls.members.filter(m=>m.name?.getText(ast)===n);assert.equal(hits.length,1,n);return hits[0].getText(ast)}).join('\n');const declarations=ast.statements.filter(ts.isVariableStatement).flatMap(s=>Array.from(s.declarationList.declarations));const constants=['RECENTLY_PLACED_ORDER_GRACE_MS','REDUCE_ONLY_LIMIT_RECONCILE_DELAYS_MS','RETIRED_ORDER_SYMBOL_TTL_MS'].map(n=>{const hits=declarations.filter(d=>d.name.getText(ast)===n);assert.equal(hits.length,1,n);return 'const '+hits[0].getText(ast)+';'}).join('\n');const context=vm.createContext({OrderType:enums.OrderType,BYBIT_BRAZIL_SITE_ID:site.BYBIT_BRAZIL_SITE_ID,virtualClock:{getCurrentTimeMs:host.now},setTimeout:host.set,clearTimeout:host.clear});vm.runInContext(ts.transpileModule(`${constants}\nclass Subject {${body}}globalThis.api=new Subject()`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);return context.api;}
function run(program){const orders=registry(),events=[],errors=Object.fromEntries(['demo','site','fallback','should','clear','set','clock','add','schedule','mark','remove','store','reconcileGet','reconcile'].map(p=>[p,{name:p+'Error',message:p+' failed',marker:42}]));let api,current,timerSeq=0,callbacks=new Map(),store=[],fetched=[],inserted=false;const keyOf=o=>Object.keys(orders).find(k=>orders[k]===o);function event(kind,refs=[],ids=[],numbers=[]){events.push({kind,recent:Array.from(api.recentlyPlacedOrders.keys()),timerKeys:Array.from(api.recentlyPlacedOrderReconciliationTimers.keys()),retired:Array.from(api.retiredOrderSymbols.keys()),refs:refs.map(keyOf),ids,numbers:numbers.map(observe)});if(current.fail===kind)throw errors[kind]}
 const host={now(){event('clock');if(current.clockMutate)Object.assign(orders[current.clockMutate.key],current.clockMutate.fields);if(current.clockAdd)api.recentlyPlacedOrders.set(orders[current.clockAdd].id,{placedAt:0,order:orders[current.clockAdd]});return Number(current.time??0)},set(callback,delay){const id='t'+timerSeq++;event('set',[],[id],[delay]);callbacks.set(id,callback);if(current.setMutate)orders.A.id=current.setMutate;return id},clear(timer){event('clear',[],[timer]);const list=api.recentlyPlacedOrderReconciliationTimers.get(current.id??orders.A.id);if(current.appendTimer&&list&&!inserted){list.push('extra');inserted=true;}if(current.spliceTimer&&list&&!inserted){list.splice(0,1);inserted=true;}if(current.replaceTimer&&!inserted){api.recentlyPlacedOrderReconciliationTimers.set(current.id,['replacement']);inserted=true;}}};api=subject(host);let demo=false,siteId;Object.defineProperty(api,'isDemo',{get(){if(current.op==='fallback')event('demo');return demo}});api.options={};Object.defineProperty(api.options,'bybitSiteId',{get(){if(current.op==='fallback')event('site');return siteId}});
 api.store={addOrUpdateOrders(rows){event('add',rows);if(current.addSeen)api.markOrderSeen(rows[0].id);},removeOrders(rows){event('remove',[],rows.map(o=>o.id))}};Object.defineProperty(api.store,'orders',{get(){event('store');if(current.storeAdd&&!inserted){const o=orders[current.storeAdd];api.recentlyPlacedOrders.set(o.id,{placedAt:0,order:o});inserted=true;}if(current.fetchedAdd&&!inserted){fetched.push(orders[current.fetchedAdd]);inserted=true;}if(current.entryOrderMutate&&!inserted){api.recentlyPlacedOrders.get(current.entryId).order=orders[current.entryOrderMutate];inserted=true;}return store;}});
 const should=api.shouldReconcileLocalReduceOnlyLimitOrder,clear=api.clearRecentlyPlacedOrderReconciliationTimers,mark=api.markOrderSeen,schedule=api.scheduleLocalReduceOnlyLimitOrderReconciliation;
 api.shouldUseLocalRestOrderLifecycleFallback=()=>{event('fallback');if(current.fallbackMutate)Object.assign(orders[current.key??'A'],current.fallbackMutate);return !!current.fallback};
 api.shouldReconcileLocalReduceOnlyLimitOrder=o=>{event('should',[o]);return current.should??should(o)};
 api.clearRecentlyPlacedOrderReconciliationTimers=id=>{event('clearMethod',[],[id]);if(current.clearMutate)orders.A.id=current.clearMutate;return clear(id)};
 api.markOrderSeen=id=>{event('mark',[],[id]);const value=mark(id);if(current.markMutate)Object.assign(orders[current.markMutate.key],current.markMutate.fields);if(current.markAdd&&!inserted){const o=orders[current.markAdd];api.recentlyPlacedOrders.set(o.id,{placedAt:0,order:o});inserted=true;}return value};
 api.scheduleLocalReduceOnlyLimitOrderReconciliation=o=>{event('schedule',[o]);return schedule(o)};
 Object.defineProperty(api,'reconcileLocalReduceOnlyLimitOrder',{get(){event('reconcileGet');if(current.getterMutate)orders.A.id=current.getterMutate;return id=>{event('reconcile',[],[id]);return {ignored:'return-token'}}}});
 const steps=[];for(current of program){events.length=0;inserted=false;demo=!!current.demo;siteId=current.site;let result;
 try{let value;const op=current.op;
 if(op==='fallback'){value=Object.getPrototypeOf(api).constructor.prototype;const original=subject(host);original.options=api.options;Object.defineProperty(original,'isDemo',{get(){event('demo');return demo}});value=original.shouldUseLocalRestOrderLifecycleFallback();}
 else if(op==='should')value=should(orders[current.key??'A']);
 else if(op==='target'){for(const[k,v]of Object.entries(current.numbers??{}))orders.A[k]=Number(v);value=api.getFilledOrderReconciliationTarget(orders.A)}
 else if(op==='record')value=api.recordRecentlyPlacedOrder(orders[current.key]);
 else if(op==='seen')value=mark(current.id);
 else if(op==='clear')value=clear(current.id);
 else if(op==='schedule')value=schedule(orders[current.key??'A']);
 else if(op==='fire')value=callbacks.get(current.timer)();
 else if(op==='retire')value=api.retireCancelledOrders((current.keys??[]).map(k=>orders[k]));
 else if(op==='resolve'){store=(current.store??[]).map(k=>orders[k]);value=api.resolveOrderSymbolForFills(current.id)}
 else if(op==='preserve'){fetched=(current.fetched??[]).map(k=>orders[k]);store=(current.store??[]).map(k=>orders[k]);value=api.preserveRecentlyPlacedOrders(fetched)}
 else if(op==='mutate')Object.assign(orders[current.key],current.fields);
 else if(op==='seed'){for(const[key,ref,time]of current.recent??[])api.recentlyPlacedOrders.set(key,{placedAt:Number(time),order:orders[ref]});for(const[key,timers]of current.timers??[])api.recentlyPlacedOrderReconciliationTimers.set(key,timers.slice());for(const[key,symbol,time]of current.retired??[])api.retiredOrderSymbols.set(key,{symbol,retiredAt:Number(time)});}
 else throw Error(op);
 result={value:observe(value),refs:Array.isArray(value)?value.map(keyOf):null,sameFetched:Array.isArray(value)?value===fetched:null};
 }catch(e){result={error:e.name,message:e.message,origin:Object.keys(errors).find(p=>errors[p]===e)??null};}
 steps.push({command:current,result,events:events.slice(),recent:Array.from(api.recentlyPlacedOrders,([id,e])=>({id,placedAt:observe(e.placedAt),ref:keyOf(e.order),order:observe(e.order)})),timers:Array.from(api.recentlyPlacedOrderReconciliationTimers,([id,timers])=>({id,timers:timers.slice()})),retired:Array.from(api.retiredOrderSymbols,([id,e])=>({id,symbol:e.symbol,retiredAt:observe(e.retiredAt)})),orders:observe(orders)});
 }return {orders:registry(),steps};}
const seed=(extra)=>({op:'seed',...extra});
const programs=[
 [{op:'preserve',fetched:['B']},{op:'retire',keys:[],fail:'clock'},{op:'seen',id:'missing'}],
 [{op:'record',key:'A',time:'100'},{op:'record',key:'B',time:'101'},{op:'record',key:'ReplacementA',time:'102'},{op:'preserve',time:'8102',fetched:['B'],store:['StoreA']},{op:'preserve',time:'8102.1'}],
 [seed({timers:[['a',['old0','old1']]]}),{op:'clear',id:'a',appendTimer:true},seed({timers:[['a',['old2','old3']]]}),{op:'clear',id:'a',spliceTimer:true},seed({timers:[['a',['old4','old5']]]}),{op:'clear',id:'a',replaceTimer:true}],
 [seed({recent:[['a','A','0']],timers:[['a',['old0','old1']]]}),{op:'seen',id:'a',fail:'clear'}, {op:'clear',id:'a'}],
 [{op:'schedule',key:'A',should:false},{op:'schedule',key:'A',should:true,setMutate:'scheduled-id'},{op:'fire',timer:'t0',getterMutate:'fire-id'},{op:'fire',timer:'t1',fail:'reconcileGet'},{op:'fire',timer:'t2',fail:'reconcile'}],
 [{op:'record',key:'Market',fail:'clock'},{op:'record',key:'A',fallback:true,should:true,clockMutate:{key:'A',fields:{id:'clock-id'}}},{op:'record',key:'B',fallback:true,fail:'add'}],
 [{op:'record',key:'A',fallback:true,should:false,addSeen:true},{op:'record',key:'B',fallback:true,fail:'schedule'},{op:'record',key:'ReplacementA',fail:'fallback'}],
 [seed({recent:[['a','A','0']],timers:[['a',['old']]]}),{op:'retire',keys:['A','B'],time:'30',fallback:true,markMutate:{key:'A',fields:{id:'retired-id',symbol:'ETHUSD'}}},{op:'resolve',id:'retired-id',time:'60030'},{op:'resolve',id:'retired-id',time:'60030.1'}],
 [seed({recent:[['a','A','0']],retired:[['a','','0'],['dead','BTCUSD','0']]}),{op:'resolve',id:'a',store:['StoreA'],fail:'clock'},{op:'resolve',id:'a',time:'60000'},{op:'resolve',id:'a',time:'60001'}],
 [seed({recent:[['a','A','0'],['b','B','0']]}),{op:'preserve',time:'1',fetched:['A'],markAdd:'Market'},{op:'preserve',time:'1',store:['StoreA'],storeAdd:'SecondStoreA'},{op:'preserve',time:'2',store:['StoreA'],fetched:['Market'],fetchedAdd:'B'}],
 [{op:'record',key:'A',fail:'clock'},seed({recent:[['a','A','0']]}),{op:'preserve',time:'1',entryId:'a',entryOrderMutate:'ReplacementA'},{op:'preserve',time:'1',fail:'store'},{op:'retire',keys:['A'],fail:'mark'},{op:'retire',keys:['A'],fallback:true,fail:'remove'}],
 [{op:'schedule',key:'A',should:true,fail:'set'}, {op:'schedule',key:'A',fail:'should'}, seed({recent:[['a','A','NaN']]}),{op:'preserve',time:'Infinity'},{op:'resolve',id:'unknown',fail:'store'}],
];
assert.equal(programs.length,12,'PRIMARY independent program floor');assert.equal(programs.reduce((n,p)=>n+p.length,0),51,'PRIMARY independent scenario step floor');
const fallback=[];for(const demo of [false,true])for(const site of [null,'BRA_BTL','bra_btl',1])fallback.push(run([{op:'fallback',demo,site}]));for(const c of [{demo:false,fail:'demo'},{demo:true,fail:'demo'},{demo:false,fail:'site'},{demo:true,fail:'site'}])fallback.push(run([{op:'fallback',...c}]));assert.equal(fallback.length,12);
const reconcile=[{fallback:false},{fallback:true},{fallback:true,fallbackMutate:{reduceOnly:true}},{fallback:true,fallbackMutate:{type:'market',reduceOnly:true}},{fallback:true,key:'B',fallbackMutate:{reduceOnly:true}},{fail:'fallback'},{fallback:false,fallbackMutate:{reduceOnly:true}},{fallback:true,fallbackMutate:{type:'limit',reduceOnly:false}}].map(c=>run([{op:'should',...c}]));assert.equal(reconcile.length,8);
const target=[{}, {amount:'1',remaining:'3',filled:'2'}, {amount:'NaN',remaining:'Infinity',filled:'1'}, {amount:'-3',remaining:'NaN'}, {amount:'-0',remaining:'-0',filled:'-0'}, {amount:'1',remaining:'2',filled:'NaN'}, {amount:'Infinity',remaining:'2',filled:'0'}, {amount:'5',remaining:'Infinity'}].map(numbers=>run([{op:'target',numbers}]));assert.equal(target.length,8);
const cases=programs.map(run);process.stdout.write(JSON.stringify({fallback,reconcile,target,cases}));
