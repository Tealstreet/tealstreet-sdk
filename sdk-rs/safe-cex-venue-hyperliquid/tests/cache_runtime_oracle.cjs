const fs=require('fs'),path=require('path'),vm=require('vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.cross-cache.ts');
const code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
function tag(v){if(v===undefined)return ['u'];if(v===null)return ['null'];if(typeof v==='string')return ['string',Array.from({length:v.length},(_,i)=>v.charCodeAt(i))];if(typeof v==='number')return ['number',Number.isNaN(v)?'nan':v];if(typeof v==='boolean')return ['bool',v];if(Array.isArray(v))return ['array',v.map(tag)];if(v instanceof Map)return ['map',[...v].map(([k,v])=>[tag(k),tag(v)])];if(v instanceof Set)return ['set',[...v].map(tag)];return ['object',Object.keys(v).map(k=>[k,tag(v[k])])];}
const entry=(type,price,amount,timestamp)=>({type,price,amount,timestamp});
const rows=[];
function run(name,family,input,body){const events=[],messages=[];let reads=0;const ticks=[...(input.ticks||[])];const storage=input.storage?{removeItem(k){events.push('remove:'+k);delete this[k]},getItem(k){return this[k]},setItem(k,v){this[k]=v}}:undefined;
const context={exports:{},Map,Set,Date:{now:()=>ticks.length?ticks.shift():40000}}; Object.defineProperty(context,'sessionStorage',{get(){events.push('storage');return storage}});
Object.defineProperty(context,'BroadcastChannel',{get(){reads++;events.push('broadcast-read');if(input.absent)return undefined;return class{constructor(name){events.push('channel:'+name);if(input.failBroadcast)throw Error('broadcast');this.postMessage=v=>messages.push(v);this.close=()=>events.push('close');if(input.failHandler)Object.defineProperty(this,'onmessage',{set(){throw Error('handler')}})}}}});
vm.runInNewContext(code,context,{filename:file});const api=context.exports;const c=new api.HyperliquidCrossCache();
if(input.pre)c.workerPrePlacementCache.set('X|buy',input.pre.map(e=>({...e})));
const result=body(api,c,input);
rows.push({name,family,input,result:tag(result),pending:tag(c.workerPendingOrdersCache),pre:tag(c.workerPrePlacementCache),seen:tag(c.workerSeenFillsSet),messages:messages.map(tag),reads,events});}
for(const [name,input]of [['normal',{storage:true}],['absent',{absent:true}],['constructor-throw',{failBroadcast:true}],['handler-throw',{failHandler:true}]])run(name,'constructor',input,()=>undefined);
for(const[action,data,key]of [['addPending',{type:'limit',price:4},'oid'],['removePending',null,'oid'],['addPrePlacement',[entry('limit',3,7,40000)],'X|buy'],['addPrePlacement',entry('stop',5,8,40000),'X|buy'],['removePrePlacement',null,'X|buy'],['seenFill',null,'f'],['addPending',false,'oid'],['seenFill',null,'']])run(action+'-'+String(key)+'-'+Array.isArray(data),'receive',{action,data,key},(_,c,i)=>c.orderCacheBroadcast.onmessage({data:{action:i.action,key:i.key,data:i.data}}));
run('receive-eviction','receive',{evict:true},(_,c)=>{for(let i=0;i<501;i++)c.orderCacheBroadcast.onmessage({data:{action:'seenFill',key:'f'+i}})});
run('add-empty','add',{args:['X','buy','limit',4,7]},(_,c,i)=>c.addPrePlacement(...i.args));
run('add-inclusive','add',{pre:[entry('boundary',1,2,10000),entry('stale',2,3,9999)],args:['X','buy','new',9,4]},(_,c,i)=>c.addPrePlacement(...i.args));
run('add-clock-jump','add',{ticks:[0,40000],args:['X','buy','new',9,4]},(_,c,i)=>c.addPrePlacement(...i.args));
const base=[entry('first',1,3,39999),entry('second',2,4,39900),entry('third',2,3,39800)];
for(const[name,pre,args]of [['price-first',base,['X','buy',2,3]],['unique-amount',base,['X','buy',99,4]],['ambiguous-amount',base,['X','buy',99,3]],['fifo',base,['X','buy']],['exclusive-age',[entry('boundary',4,7,10000)],['X','buy']],['inside-age',[entry('inside',4,7,10001)],['X','buy']],['empty',[],['X','buy']],['absent',null,['X','buy']],['zero-amount',[entry('first',1,3,40000),entry('zero',2,0,40000)],['X','buy',99,0]]])run(name,'match',{pre,args},(_,c,i)=>c.matchPrePlacement(...i.args));
const cx={exports:{},Map,Set,Date:{now:()=>40000}};vm.runInNewContext(code,cx);const api=cx.exports;const a=api.getCrossCache(),b=api.getCrossCache();const id={};const before=api.hasSeenFill(id),mark=api.markFillSeen(id),after=api.hasSeenFill(id);a.dispose();rows.push({name:'module',family:'module',same:a===b,afterDispose:a===api.getCrossCache(),before:tag(before),after:tag(after),mark:tag(mark)});
const prototype=api.HyperliquidCrossCache.prototype;rows.push({name:'class',family:'class',names:Object.getOwnPropertyNames(prototype),enumerable:Object.keys(prototype),constructor:prototype.constructor===api.HyperliquidCrossCache});
process.stdout.write(JSON.stringify(rows));
