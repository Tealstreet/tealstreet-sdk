const futuresWs=path.join(src,'exchanges/kraken/kraken.ws-public.ts'),spotWs=path.join(src,'exchanges/kraken/krakenspot.ws-public.ts');
const units=text=>Array.from({length:text.length},(_,i)=>text.charCodeAt(i));
function feedCase(family,config){
 const trace=[];const original=new Error('feed stats failure');original.name='FeedError';
 const proto={},handlers=Object.create(proto);
 for(const [object,rows]of[[proto,config.proto||[]],[handlers,config.own||[]]])for(const row of rows)Object.defineProperty(object,String.fromCharCode(...row.key),{enumerable:true,get(){trace.push({kind:'read',key:row.key});if(row.fail)throw original;return row.len===null?undefined:Array(row.len).fill(null)}});
 const feed=String.fromCharCode(...config.feed);
 const fn=method(family==='Futures'?futuresWs:spotWs,family==='Futures'?'KrakenPublicWebsocket':'KrakenSpotPublicWebsocket','clearFeedStatsIfIdle',{}, {cleanupMessageHandlerStats(value){trace.push({kind:'cleanup',feed:units(value)});if(config.cleanupFail)throw original}});
 let failure=null;try{fn(feed,handlers)}catch(e){failure={name:e.name,message:e.message,identity:e===original}}
 return {config,trace,failure};
}
function feedCorpus(){
 const entry=(key,len,fail=false)=>({key:units(key),len,fail});
 const specs=[{own:[]},{own:[entry('one',1)]},{own:[entry('zero',0),entry('missing',null)]},{own:[entry('first',1),entry('later',0,true)]},{own:[entry('first',0),entry('later',0,true)]},{own:[],cleanupFail:true},{own:[entry('10',0),entry('2',1),entry('z',0,true)]},{own:[entry('own',0)],proto:[entry('inherited',1)]},{own:[entry('same',0)],proto:[entry('same',1),entry('other',0)]},{own:[entry('constructor',0),entry('\ud800',0)]},{own:[entry('a',0)],proto:[entry('inherited',0)]},{own:[entry('a',2)],cleanupFail:true}];
 return Object.fromEntries(['Futures','Spot'].map(family=>[family,specs.map((config,i)=>feedCase(family,{...config,feed:units(i===9?'\ud800':'book')}))]));
}
