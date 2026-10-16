'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../../packages/safe-cex/src'),cache=new Map();
function load(file,extra=[]){const key=file+extra.join(',');if(cache.has(key))return cache.get(key);const exports={};cache.set(key,exports);const code=ts.transpileModule(fs.readFileSync(file,'utf8')+(extra.length?'\nexport { '+extra.join(',')+' };':''),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;vm.runInNewContext(code,{exports,require(name){assert.ok(name.startsWith('.'),name);return load(path.resolve(path.dirname(file),name+'.ts'));}});return exports;}
const recoveryFile=path.join(root,'exchanges/woo/woo.v3.orderbook-recovery.ts');
const api=load(recoveryFile,['isRecord','requirePositiveTimestamp','requireNumericLevel','normalizeSide','normalizeUpdate','snapshotLevels']);
const publicApi=load(path.join(root,'exchanges/woo/woo.v3.public.ts'));
function native(v){if(typeof v==='number')return {$number:Object.is(v,-0)?'-0':String(v)};if(Array.isArray(v))return Array.from(v,native);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).filter(([,x])=>x!==undefined).map(([k,x])=>[k,native(x)]));return v;}
function raw(v){if(v===undefined)return undefined;if(typeof v==='number')return Object.is(v,-0)?'-0':v===Infinity?'1e999':v===-Infinity?'-1e999':String(v);if(Array.isArray(v))return '['+v.map(raw).join(',')+']';if(v&&typeof v==='object')return '{'+Object.entries(v).filter(([,x])=>x!==undefined).map(([k,x])=>JSON.stringify(k)+':'+raw(x)).join(',')+'}';return JSON.stringify(v);}
const capture=fn=>{try{return {ok:native(fn())};}catch(e){return {error:{name:e.name,message:e.message}};}};
assert.equal(api.WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES,500);
const values=[undefined,null,[],{},0,-0,1,0.5,-1,9007199254740991,9007199254740992,Infinity,'1',true];
const records=values.map(v=>({input:raw(v),ok:api.isRecord(v)}));
const timestamps=values.map(v=>({input:raw(v),...capture(()=>api.requirePositiveTimestamp(v,'probe'))}));
const huge='9'.repeat(400),tiny='0.'+'0'.repeat(400)+'1';
const levelValues=[['001.20','0'],['.5','2.'],['1.', '.25'],['5'+'0'.repeat(323),'1'],['1','0.'+'0'.repeat(323)+'5'],['0','1'],['-0.1','1'],['1','-0'],['1','1e2'],['1e2','1'],['+1','1'],[' 1','1'],['.','1'],['1',''],['1','1.2.3'],['1',3],['1'],['1','2','3'],null,{},[tiny,'1'],['1',tiny],[huge,'1'],['1',huge]];
const levels=levelValues.map(v=>({input:raw(v),...capture(()=>api.requireNumericLevel(v,'probe'))}));assert.equal(levels.length,24);
const sideValues=[[],[['1.1','2.2'],['3.3','0']],[['1','2'],['0','bad']],[[1,2]],null,{}];
const sides=sideValues.map(v=>({input:raw(v),...capture(()=>api.normalizeSide(v,'probe'))}));assert.equal(sides.length,6);
const symbol='PERP_BTC_USDT',delta=(prev=100,time=101,asks=[['102','5']],bids=[['99','7']])=>({symbol,previousTimestamp:prev,timestamp:time,asks,bids});
const updateValues=[delta(),delta(1,9007199254740991,[['001.20','0']],[['.5','2.']]),{...delta(),symbol:'BAD',previousTimestamp:0},{...delta(),previousTimestamp:0,timestamp:0},delta(100,100),delta(102,101),{...delta(),asks:[[1,2]],bids:[[1,2]]},{...delta(),asks:null,bids:null},{...delta(),previousTimestamp:'100'},{...delta(),timestamp:Infinity},null,[]];
const updates=updateValues.map(v=>({input:raw(v),...capture(()=>api.normalizeUpdate(v,symbol))}));assert.equal(updates.length,12);
const snapshot=(time=100,bids=[{price:'99',quantity:'2'},{price:'98',quantity:'3'}],asks=[{price:'103',quantity:'4'},{price:'102',quantity:'5'}])=>({success:true,timestamp:time,data:{bids,asks}});
const snapNative=publicApi.mapWooV3OrderBook(publicApi.decodeWooV3OrderBook(snapshot()));
const snapshotSides=['bids','asks'].map(side=>({side,ok:native(api.snapshotLevels(snapNative,side))}));
const constructors=[['',50],[3,50],[symbol,0],[symbol,'50'],[symbol,50],[symbol,200],[symbol,500],[symbol,Infinity],[null,500],[symbol,undefined]].map(([s,d])=>({symbol:raw(s),depth:raw(d),...capture(()=>new api.WooV3OrderBookRecoveryCoordinator(s,d).requiresFreshSnapshot)}));assert.equal(constructors.length,10);
const B=()=>({op:'begin'}),P=(input=delta())=>({op:'push',input}),S=(input=snapshot(),token='latest')=>({op:'snapshot',input,token}),R=()=>({op:'reconnect'});
const traces=[
 ['late-join',[B(),S(),P()]],
 ['buffered-join',[P(delta(99,100)),P(),P(delta(101,103,[['102','0'],['104','6']],[['99','9'],['97','8']])),B(),S()]],
 ['buffer-duplicate',[P(),P(),B(),S()]],
 ['buffer-conflict',[P(),P(delta(100,101,[['102.0','5']],[['99','7']])),B(),S()]],
 ['buffer-out-of-order',[P(delta(101,102)),P(),B(),S()]],
 ['missing-join-queued',[P(delta(101,102)),B(),S()]],
 ['missing-join-late',[B(),S(),P(delta(101,102))]],
 ['sequence-gap-queued',[P(),P(delta(102,104)),B(),S()]],
 ['sequence-gap-live',[B(),S(),P(),P(delta(102,104))]],
 ['joined-duplicate-stale',[B(),S(),P(),P(),P(delta(99,100)),P(delta(101,102))]],
 ['joined-conflict',[B(),S(),P(),P(delta(100,101,[['102','5.0']],[['99','7']]))]],
 ['waiting-stale',[B(),S(),P(delta(98,99)),P(delta(99,100)),P()]],
 ['request-replacement',[B(),B(),S(snapshot(),1),S(snapshot(),2),S(null,2),P()]],
 ['request-typed-refusal',[B(),S(null,'1'),S(null,0),S(null,1.5),S(null,Infinity),S(snapshot(),1),P()]],
 ['invalid-snapshot-consumes',[B(),S(null),S(snapshot()),B(),S(),P()]],
 ['invalid-snapshot-shapes',[B(),S({...snapshot(),success:false}),B(),S(snapshot(0)),B(),S(snapshot(100,[{price:'99',quantity:'0'}])),B(),S()]],
 ['invalid-delta-resets',[P(),B(),P({...delta(),symbol:'BAD'}),S(),B(),S(),P()]],
 ['overflow-precedes-invalid',[B(),P({symbol:'BAD',asks:Array.from({length:51},()=>null),bids:null}),S()]],
 ['snapshot-overflow',[B(),S(snapshot(100,Array.from({length:51},(_,i)=>({price:String(i+1),quantity:'1'})))),B(),S()]],
 ['reconnect-retains-nonce',[B(),S(),P(),R(),S(snapshot(),1),B(),S(),P()]],
 ['numeric-price-collision',[B(),S(snapshot(100,[{price:'99',quantity:'1'},{price:'99.0',quantity:'4'}])),P(delta(100,101,[],[['99.00','0'],['98.5','3']]))]],
 ['decimal-snapshot',[B(),S(snapshot(100,[{price:'0x10',quantity:'2'}],[{price:'1e2',quantity:'3'}])),P(delta(100,101,[['100','4']],[['16','5']]))]],
 ['invalid-delta-level',[B(),S(),P(delta(100,101,[['1',tiny]],[])),B(),S(),P()]],
 ['buffer-ceiling',[...Array.from({length:501},(_,i)=>P(delta(100+i,101+i,[],[]))),B(),S()]],
 ['cross-depth-200',[B(),P(delta(100,101,Array.from({length:200},(_,i)=>[String(i+1),'1']),[])),S(),P(delta(101,102,Array.from({length:201},()=>['1','1']),[]))]],
];assert.equal(traces.length,25);
const stepCounts=[3,5,4,4,4,3,3,4,4,6,4,5,6,7,6,8,7,3,4,8,3,3,6,503,4];assert.deepEqual(traces.map(([,ops])=>ops.length),stepCounts,"independent trace step floors");
const scenarios=traces.map(([name,ops])=>{const depth=name==='cross-depth-200'?200:50,c=new api.WooV3OrderBookRecoveryCoordinator(symbol,depth);let latest;const steps=ops.map(step=>{let result;if(step.op==='begin'){result=c.beginSnapshotRequest();latest=result;}else if(step.op==='push')result=c.pushUpdate(step.input);else if(step.op==='snapshot')result=c.acceptRestSnapshot(step.input,step.token==='latest'?latest:step.token);else result=c.reconnect();if(result&&typeof result==='object')assert.ok(Object.isFrozen(result));if(result?.publication){assert.ok(Object.isFrozen(result.publication));for(const side of ['bids','asks'])for(const level of result.publication[side])assert.ok(Object.isFrozen(level));}return {op:step.op,input:raw(step.input),token:step.op==='snapshot'?raw(step.token==='latest'?latest:step.token):undefined,expected:native(result),requiresFreshSnapshot:c.requiresFreshSnapshot};});return {name,depth,steps};});
const nonce=new api.WooV3OrderBookRecoveryCoordinator(symbol,50);nonce.nextSnapshotRequestToken=9007199254740991;const tokenBoundary=[capture(()=>nonce.beginSnapshotRequest()),capture(()=>nonce.beginSnapshotRequest())];
const owned=new api.WooV3OrderBookRecoveryCoordinator(symbol,50),t=owned.beginSnapshotRequest();owned.acceptRestSnapshot(snapshot(),t);const first=owned.pushUpdate(delta());const saved=native(first.publication);owned.pushUpdate(delta(101,102,[['102','99']],[['99','100']]));assert.deepEqual(native(first.publication),saved,'earlier publication snapshot remains stable');first.publication.asks.push({price:999,amount:999,total:999});const afterArrayMutation=owned.pushUpdate(delta(102,103,[],[]));assert.equal(afterArrayMutation.publication.asks.length,2,'publication side arrays do not alias internal book');
console.log(JSON.stringify({records,timestamps,levels,sides,updates,snapshotSides,constructors,scenarios,tokenBoundary,publicationOwnership:{first:saved,afterArrayMutation:native(afterArrayMutation.publication)}}));
