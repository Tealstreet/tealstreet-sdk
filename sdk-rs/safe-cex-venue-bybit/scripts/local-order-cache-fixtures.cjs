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
function subject(file,name,clock){
 const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/bybit',file),'utf8');
 const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
 const cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text===name);assert(cls);
 const names=['recentlyPlacedOrders','shouldUseLocalRestOrderLifecycleFallback','markOrderSeen','recordRecentlyPlacedOrder','retireCancelledOrders','preserveRecentlyPlacedOrders'];
 const body=names.map(n=>{const found=cls.members.filter(m=>m.name?.getText(ast)===n);assert.equal(found.length,1,`${name}.${n}`);return found[0].getText(ast);}).join('\n');
 const grace=ast.statements.filter(ts.isVariableStatement).flatMap(s=>Array.from(s.declarationList.declarations)).find(d=>d.name.getText(ast)==='RECENTLY_PLACED_ORDER_GRACE_MS');assert(grace);
 const context=vm.createContext({OrderType:enums.OrderType,BYBIT_BRAZIL_SITE_ID:site.BYBIT_BRAZIL_SITE_ID,virtualClock:clock});
 vm.runInContext(ts.transpileModule(`const ${grace.getText(ast)}; class Subject { ${body} } globalThis.subject=new Subject();`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 return context.subject;
}
function observe(value){
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {number:Number.isNaN(value)?'nan':b.toString('hex')};}
 if(Array.isArray(value))return Array.from(value,observe);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
 return value;
}
const order=(id,type='limit',price=23.5)=>({id,status:'open',symbol:'BTCUSD',type,side:'buy',price,amount:7.25,filled:1.5,remaining:5.75,reduceOnly:false,timeInForce:'GoodTillCancel',lastUpdate:123.5});
const registry=()=>({A:order('a'),B:order('b','conditional'),ReplacementA:order('a','limit',41.25),StoreA:order('a','limit',53.75),SecondStoreA:order('a','limit',67.125),Market:order('m','market')});
const rec=(key,time,site='BRA_BTL',extra={})=>({op:'record',key,time:String(time),site,...extra});
const pres=(time,fetched=[],store=[],extra={})=>({op:'preserve',time:String(time),fetched,store,...extra});
const retire=(ids,site='BRA_BTL',host='ok')=>({op:'retire',ids,site,host});
const programs=[
 [pres(0,['B']),retire([], 'BRA_BTL','fail'),{op:'seen',id:'missing'}],
 [rec('A',100,null),pres(8100),pres(8100.0001)],
 [rec('A',100),rec('B',101),pres(8100,['B'],['StoreA','SecondStoreA']),{op:'seen',id:'a'},pres(8101,['A'])],
 [rec('A',0,null),rec('B',1,null),rec('ReplacementA',2,null),pres(7000,['Market']),rec('Market',8,'BRA_BTL',{host:'fail'})],
 [rec('A',10,null),{op:'mutate',key:'A',fields:{amount:9,remaining:7,filled:2,price:77.5,lastUpdate:321}},pres(11)],
 [rec('Market',0,'BRA_BTL',{host:'fail'}),pres(9000,['Market'])],
 [rec('A',7,'BRA_BTL',{host:'fail'}),pres(8),rec('B',9,'BRA_BTL',{host:'seen'}),pres(10)],
 [rec('A',0),rec('B',1,null),retire(['a','unknown','a'],'BRA_BTL','fail'),pres(2)],
 [rec('A',0,null),rec('B',1,null),retire(['b','a']),pres(10000)],
 [rec('A','NaN',null),rec('B','Infinity',null),pres('Infinity'),pres('-Infinity'),rec('ReplacementA','-0',null),pres('-0')],
 [rec('A',10,null,{clockMutate:{key:'A',fields:{id:'renamed',type:'market'}}}),pres(11,['ReplacementA'])],
 [rec('A',0,null),pres(1,[],[],{clockSeen:'a'})],
 [rec('A',0,null,{clockSite:'BRA_BTL'}),pres(1)],
 [rec('A',0,null),pres(1,[],['SecondStoreA'],{clockStore:['StoreA']})],
 [rec('A',0,null),pres(1,[],['StoreA'],{observeStore:true})],
];
assert.equal(programs.length,15,'CACHE independent program floor');
assert.equal(programs.reduce((n,p)=>n+p.length,0),49,'CACHE independent step floor');
const cases=[],fallback=[];
for(const [family,file,name]of[['inverse','bybitinverse.exchange.ts','BybitInverseExchange'],['spot','bybitspot.exchange.ts','BybitSpotExchange']]){
 const basic=subject(file,name,{getCurrentTimeMs(){throw Error('unexpected clock');}});
 for(const value of [undefined,null,true,false,0,1,'BRA_BTL','bra_btl','BRA_BTL ',['BRA_BTL'],{}]){
  basic.options={bybitSiteId:value};fallback.push({family,raw:value===undefined?null:JSON.stringify(value),result:basic.shouldUseLocalRestOrderLifecycleFallback()});
 }
 for(let index=0;index<programs.length;index++){
  const orders=registry(),events=[];let current;
  let api;
  const keyOf=o=>Object.keys(orders).find(key=>orders[key]===o);
  const event=(kind,refs=[])=>events.push({kind,keys:Array.from(api.recentlyPlacedOrders.keys()),orders:refs.map(keyOf)});
  api=subject(file,name,{getCurrentTimeMs(){event('clock');if(current.clockMutate)Object.assign(orders[current.clockMutate.key],current.clockMutate.fields);if(current.clockSeen)api.markOrderSeen(current.clockSeen);if(current.clockSite!==undefined)api.options.bybitSiteId=current.clockSite;if(current.clockStore)api.store.orders=current.clockStore.map(key=>orders[key]);return Number(current.time);}});
  api.options={};
  api.store={orders:[],addOrUpdateOrders(rows){event('add',rows);if(current.host==='seen')api.markOrderSeen(rows[0].id);if(current.host==='fail')throw Object.assign(new Error('store callback failed'),{name:'StoreError',marker:41});},removeOrders(rows){events.push({kind:'remove',keys:Array.from(api.recentlyPlacedOrders.keys()),orders:rows.map(o=>o.id)});if(current.host==='fail')throw Object.assign(new Error('store callback failed'),{name:'StoreError',marker:41});}};
  let storedOrders=[];
  Object.defineProperty(api.store,'orders',{get(){if(current.observeStore)event('storeRead');return storedOrders;},set(value){storedOrders=value;}});
  const steps=[];
  for(current of programs[index]){
   events.length=0;api.options.bybitSiteId=current.site;
   let result={ok:true};
   try{
    if(current.op==='record')api.recordRecentlyPlacedOrder(orders[current.key]);
    else if(current.op==='seen')api.markOrderSeen(current.id);
    else if(current.op==='retire')api.retireCancelledOrders(current.ids.map(id=>({id,symbol:'BTCUSD'})));
    else if(current.op==='mutate')Object.assign(orders[current.key],current.fields);
    else if(current.op==='preserve'){
     const fetched=current.fetched.map(key=>orders[key]);api.store.orders=current.store.map(key=>orders[key]);
     const preserved=api.preserveRecentlyPlacedOrders(fetched);
     result={orders:preserved.map(o=>({ref:keyOf(o),value:observe(o)})),sameArray:preserved===fetched};
    }else throw Error('unknown command');
   }catch(error){result={error:error.name,message:error.message,marker:error.marker};}
   const cache=Array.from(api.recentlyPlacedOrders,([key,entry])=>({key,placedAt:observe(entry.placedAt),ref:keyOf(entry.order),order:observe(entry.order)}));
   steps.push({command:current,result,cache,events:events.slice()});
  }
  cases.push({family,index,orders:registry(),steps});
 }
}
assert.equal(fallback.length,22,'CACHE independent fallback floor');
assert.equal(cases.length,30,'CACHE independent scenario floor');
assert.equal(cases.reduce((n,c)=>n+c.steps.length,0),98,'CACHE independent observation floor');
process.stdout.write(JSON.stringify({fallback,cases}));
