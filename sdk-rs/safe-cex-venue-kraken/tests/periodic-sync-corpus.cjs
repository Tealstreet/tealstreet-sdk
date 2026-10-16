const countValue=tag=>({zero:0,negativeZero:-0,one:1,nineteen:19,twenty:20,forty:40,negativeTwenty:-20,fraction:20.5,nan:NaN,infinity:Infinity}[tag]);
const pump=async()=>{for(let i=0;i<8;i++)await Promise.resolve()};
async function tickCase(family,config){
 const trace=[],pending={},loops=[],original=new Error(config.emptyMessage?'':'tick failure');original.name='TickError';
 let disposed=!!config.disposed,done=false,failure=null,failed=false;
 const batches={orders:{marker:17.25},positions:{marker:19.5},balance:config.falseBalance?false:{marker:23.75}};
 const fail=point=>{if(config.fail===point&&!failed){failed=true;throw original}};
 const touch=point=>{if(config.disposeAt===point)disposed=true};
 const store={loaded:{markets:true,tickers:false,balance:false,positions:false,orders:false},addOrUpdateOrdersFull(value){trace.push({kind:'full',which:'orders',identity:value===batches.orders,marker:capture(value.marker)});fail('full-orders');touch('full-orders');if(config.changeLoaded)store.loaded={...store.loaded,markets:false,tickers:true}},addOrUpdatePositionsFull(value){trace.push({kind:'full',which:'positions',identity:value===batches.positions,marker:capture(value.marker)});fail('full-positions');touch('full-positions')},update(value){const which=value.balance?'balance':'loaded';trace.push({kind:which,flags:{...value.loaded},...(value.balance?{identity:value.balance===batches.balance,marker:capture(value.balance.marker)}:{})});fail(value.balance?'balance-update':'loaded-update');store.loaded=value.loaded;touch(which)}};
 const fetch=which=>{trace.push({kind:'fetch',which});fail('sync-'+which);if(config.immediate)return Promise.resolve(batches[which]);return new Promise((resolve,reject)=>{pending[which]={resolve,reject}})};
 const receiver={get isDisposed(){return disposed},store,fetchLeverageSettings:()=>fetch('leverage'),fetchOrders:()=>fetch('orders'),fetchPositions:()=>fetch('positions'),fetchBalance:()=>fetch('balance'),emitAndLogError(message){trace.push({kind:'error',message});touch('error');if(config.fail==='emit')throw original}};
 const loop=(callback,delay)=>{trace.push({kind:'loop',delay:capture(delay)});loops.push(callback);fail('loop')};
 const fn=method(family==='Futures'?futuresFile:spotFile,family==='Futures'?'KrakenExchange':'KrakenSpotExchange','tick',{loop},receiver);receiver.tick=fn;
 const start=promise=>{done=false;promise.then(()=>{done=true},error=>{done=true;failure={name:error.name,message:error.message,identity:error===original}})};
 start(fn(countValue(config.count)));await pump();const checkpoints=[];
 const snapshot=()=>({events:trace.length,done,disposed,flags:{...store.loaded}});checkpoints.push(snapshot());
 for(let pass=0;pass<(config.recurse?2:1);pass++){
  if(pass){if(config.replaceTick)receiver.tick=function(next){trace.push({kind:'override',count:capture(next),identity:this===receiver});return Promise.resolve()};const callback=loops.shift();if(callback)start(callback());await pump();checkpoints.push(snapshot())}
  for(const which of config.order||['leverage','orders','positions','balance']){
   const task=pending[which];delete pending[which];if(task){touch('release-'+which);if(config.fail===which||(config.fail==='emit'&&which==='orders'))task.reject(original);else task.resolve(batches[which]);}await pump();checkpoints.push(snapshot());
  }
 }
 return {config,trace,checkpoints,failure};
}
async function periodicCorpus(){
 const configs=[{count:'missing'},...['zero','negativeZero','one','nineteen','twenty','forty','negativeTwenty','fraction','nan','infinity'].map(count=>({count})),{count:'zero',disposed:true},{count:'zero',falseBalance:true},{count:'zero',changeLoaded:true},{count:'zero',order:['leverage','balance','positions','orders']},{count:'nineteen',recurse:true},{count:'nineteen',recurse:true,replaceTick:true},{count:'zero',immediate:true},...['leverage','orders','positions','balance','sync-orders','sync-positions','sync-balance','full-orders','full-positions','loaded-update','balance-update','loop','emit'].map(fail=>({count:'zero',fail})),{count:'zero',fail:'orders',emptyMessage:true},...['release-leverage','release-orders','full-orders','full-positions','balance','error'].map(disposeAt=>({count:'zero',disposeAt,...(disposeAt==='error'?{fail:'orders'}:{})}))];
 const result={};for(const family of ['Futures','Spot']){result[family]=[];for(const config of configs)result[family].push(await tickCase(family,config))}return result;
}
