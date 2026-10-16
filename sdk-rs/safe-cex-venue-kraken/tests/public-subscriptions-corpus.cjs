const units=text=>Array.from({length:text.length},(_,i)=>text.charCodeAt(i)),text=units=>String.fromCharCode(...units);
function subscriptionCase(family,config){
 const trace=[],original=new Error('subscription failure');original.name='SubscriptionError';let sends=0,disposed=!!config.disposed;
 const book={},ohlc={},proto={};Object.setPrototypeOf(book,proto);
 const row=(object,key,len)=>{Object.defineProperty(object,key,{enumerable:true,configurable:true,get(){trace.push({kind:'length',map:object===ohlc?'ohlc':'book',key:units(key)});if(config.fail==='length-'+key)throw original;return len===null?undefined:Array(len).fill(null)}})};
 for(const[key,len]of config.books||[['B',1],['idle',0],['absent',null]])row(book,key,len);
 for(const[key,len]of config.ohlc||[['BTC/USD::1h',1],['idle::5m',0]])row(ohlc,key,len);
 row(proto,'inherited',1);
 const proxy=(object,map)=>new Proxy(object,{ownKeys(target){trace.push({kind:'keys',map});if(config.fail==='keys-'+map)throw original;return Reflect.ownKeys(target)}});
 const products=Array.from({length:config.count??1},(_,i)=>i===0?'雪/\ud800':'P'+i);
 const receiver={get isDisposed(){return disposed},parent:{store:{markets:products.map(symbol=>({symbol}))}},orderBookHandlers:proxy(book,'book'),ohlcvHandlers:proxy(ohlc,'ohlc'),getTickerProductIds(){trace.push({kind:'products'});if(config.fail==='products')throw original;return products.slice()},send(wire){trace.push({kind:'send',wire});sends++;if(config.disposeSend)disposed=true;if(config.mutateTicker&&sends===1){receiver.parent.store.markets.push({symbol:'LATE'});row(book,'LATE',1)}if(config.mutateBook&&sends===(config.count?Math.ceil(config.count/50):1)+1){delete book.idle;row(book,'ADDED',1);row(ohlc,'ADDED::3m',1)}if(config.fail==='send-'+sends||(config.fail==='console'&&sends===2))throw original}};
 const prepared=frame=>{if(family==='Futures')return {kind:'frame',variant:'futures',event:frame.event,feed:frame.feed,symbols:frame.product_ids.map(units)};
 const p=frame.params;const result={kind:'frame',variant:p.channel,method:frame.method,channel:p.channel,symbols:p.symbol.map(units)};
 if(p.channel==='ticker')result.eventTrigger=p.event_trigger;if(p.channel==='book')result.depth=capture(p.depth);
 if(p.channel==='ohlc')result.interval=typeof p.interval==='number'?{number:capture(p.interval).number}:{inherited:String(p.interval),identity: typeof p.interval==='object'?p.interval===Object.getPrototypeOf(spotTypes.INTERVAL):p.interval===Object.getPrototypeOf(spotTypes.INTERVAL)[(config.ohlc||[]).map(([k])=>k.split('::')[1]).find(tf=>spotTypes.INTERVAL[tf]===p.interval)]};
 return result};
 const json={stringify(frame){trace.push(prepared(frame));if(config.fail==='stringify')throw original;return JSON.stringify(frame)}};
 const errorConsole={error(label,symbol,error){trace.push({kind:'error',label,key:units(symbol),identity:error===original});if(config.fail==='console')throw original}};
 const file=path.join(src,'exchanges/kraken',family==='Futures'?'kraken.ws-public.ts':'krakenspot.ws-public.ts');
 const fn=method(file,family==='Futures'?'KrakenPublicWebsocket':'KrakenSpotPublicWebsocket','onOpen',{JSON:json,console:errorConsole,INTERVAL:spotTypes.INTERVAL},receiver);
 let failure=null;try{fn()}catch(error){failure={name:error.name,message:error.message,identity:error===original}};
 return {config:{...config,books:config.books?.map(([key,len])=>[units(key),len]),ohlc:config.ohlc?.map(([key,len])=>[units(key),len])},trace,failure,disposed};
}
function subscriptionCorpus(){
 const configs=[...[0,1,49,50,51,100,101].map(count=>({count})),{disposed:true},{count:51,disposeSend:true},{count:51,mutateTicker:true},{mutateBook:true},{books:[['10',1],['2',1],['z',1],['constructor',0]]},{books:[['B',0]],ohlc:[]},...['products','keys-book','keys-ohlc','length-B','length-BTC/USD::1h','send-1','send-2','send-3','stringify'].map(fail=>({fail})),{fail:'console'},...['1m','3m','5m','15m','30m','1h','2h','4h','6h','12h','1d','1w','unknown','constructor','toString','__proto__','\ud800','','5m::ignored'].map(tf=>({ohlc:[['雪::'+tf,1]]})),{ohlc:[['no-delimiter',1],['::',1]]}];
 return Object.fromEntries(['Futures','Spot'].map(family=>[family,configs.map(config=>subscriptionCase(family,config))]));
}
