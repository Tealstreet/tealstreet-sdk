function dispatchCase(kind,config){
 const trace=[],original=new Error('dispatch failure');original.name='DispatchError';let type=config.type??'snapshot',appended=false;
 const values={text:'雪/\ud800',empty:'',false:false,zero:0,negativeZero:-0,nan:NaN,null:null,undefined:undefined,array:[],object:{marker:17.25}};
 const fail=point=>{if(config.fail===point)throw original};
 const make=(tag,id)=>{const item={id,marker:19.5};Object.defineProperty(item,'symbol',{get(){trace.push({kind:'symbol',id});fail('symbol');return values[tag]}});return item};
 const items=config.empty?[]:(config.tags||['text','empty','false','zero','negativeZero','nan','null','undefined','array','object']).map((tag,i)=>make(tag,i+1));
 if(config.nullItem)items.push(null);
 const message={get data(){trace.push({kind:'data'});fail('data');return config.notArray?{}:items},get type(){trace.push({kind:'type'});fail('type');return type}};
 const dispatch=(method,symbol,item)=>{trace.push({kind:'dispatch',method,id:item.id,itemIdentity:items.includes(item),symbolIdentity:Object.values(values).some(value=>value===symbol),marker:capture(item.marker)});fail(method);if(config.flipType)type='update';if(config.append&&!appended){appended=true;items.push(make('text',31))}};
 const receiver={handleBookSnapshot:(symbol,item)=>dispatch('snapshot',symbol,item),handleBookDelta:(symbol,item)=>dispatch('delta',symbol,item),parent:{refreshBalance(){trace.push({kind:'refresh'});fail('refresh');return new Promise(()=>{})}}};
 const file=path.join(src,'exchanges/kraken',kind==='book'?'krakenspot.ws-public.ts':'krakenspot.ws-private.ts'),cls=kind==='book'?'KrakenSpotPublicWebsocket':'KrakenSpotPrivateWebsocket';
 const fn=method(file,cls,kind==='book'?'handleBook':'handleBalances',{},receiver);
 let failure=null,returned=false;try{const result=fn(message);returned=result===undefined}catch(error){failure={name:error.name,message:error.message,identity:error===original}};
 return {config,trace,failure,returned};
}
function dispatchCorpus(){
 const book=[{},{type:'update'},{type:'unknown'},{empty:true},{notArray:true},{flipType:true},{append:true},{flipType:true,append:true},{nullItem:true},...['data','symbol','type','snapshot','delta'].map(fail=>({fail,type:fail==='delta'?'update':'snapshot'}))];
 const balances=[{},{empty:true},{notArray:true},{nullItem:true},{fail:'data'},{fail:'refresh'}];
 return {book:book.map(config=>dispatchCase('book',config)),balances:balances.map(config=>dispatchCase('balances',config))};
}
