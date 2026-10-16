globalThis.spotBalanceSupplement=async()=>{
 const base={result:{ZUSD:'11.25',USDT:'13.5',USDC:'17.75',XXBT:'2.25',XETH:'3.5','ETH.B':'999'}};
 const tickers=[{symbol:'BTC/USDT',last:19.25},{symbol:'BTC/USD',last:23.5},{symbol:'ETH/USD',last:29.75}];
 const specs=[];
 const add=(label,data=base,extra={})=>specs.push({label,raw:JSON.stringify(data),...extra});
 add('baseline');add('reverse-tickers',base,{reverse:true});
 for(const data of [null,false,{}, {result:null},{result:false},{result:[]},{result:['2.5','3.75']},{result:'12'},{result:{USD:'0',EUR:'-1',JPY:'NaN',CAD:'Infinity'}},{result:{ZUSD:'11.25',USD:'17.5',XBT:'3.25',XXBT:'5.75'}},{result:{constructor:'2.5',toString:'3.75'}},{result:{'2':'5.25','1':'7.5'}},{result:{USD:0,USDT:false,USDC:null,EUR:[],JPY:{}}},{result:{USD:{toString:7,valueOf:9}}}])add('shape-'+specs.length,data);
 add('undefined',base,{raw:null});
 for(const last of [0,-1,'NaN'])add('ticker-'+last,base,{last});
 for(const mode of ['sync','async'])for(const replace of [false,true])for(const emitFail of [false,true])add('error-'+mode+'-'+replace+'-'+emitFail,base,{mode,replace,emitFail});
 add("late-tickers",base,{late:true});
 const cases=[];
 const fallback=n=>({free:n+1,used:n+2,total:n+3,upnl:n+4,currencies:{OLD:{free:n+5,used:n+6,total:n+7,upnl:n+8,info:{tag:'old'}}},info:{tag:'fallback'}});
 for(const spec of specs){
  const calls=[],emits=[];const original=new Error('balance read failure'),emitError=new Error('balance emit failure');
  const rows=tickers.map(t=>({...t,last:Object.hasOwn(spec,'last')?Number(spec.last):t.last}));if(spec.reverse)rows.reverse();
  const receiver={store:{tickers:rows,balance:fallback(101.25)},xhr:{post(endpoint,body){calls.push({endpoint,body});if(spec.mode==='sync')throw original;if(spec.mode==='async')return Promise.reject(original);return Promise.resolve({data:value(spec.raw)});}},emitAndLogError(message){emits.push(message);if(spec.replace)this.store.balance=fallback(211.5);if(spec.emitFail)throw emitError;}};
  const fn=method(spotFile,'KrakenSpotExchange','fetchBalance',{ENDPOINTS:spotTypes.ENDPOINTS,normalizeAsset:spotTypes.normalizeAsset},receiver);
  try {const pending=fn();if(spec.late)receiver.store.tickers=rows.map(t=>({...t,last:41.25}));const result=await pending;cases.push({...spec,calls,emits,tickers:capture(rows),result:capture(result),sameFallback:result===receiver.store.balance,storeAfter:capture(receiver.store.balance)});}
  catch(error){cases.push({...spec,calls,emits,tickers:capture(rows),error:error.message,sameError:error===emitError,storeAfter:capture(receiver.store.balance)});}
 }
 const raw=JSON.stringify({result:'😀'});
 const receiver={store:{tickers:[],balance:fallback(101.25)},xhr:{post:async()=>({data:JSON.parse(raw)})},emitAndLogError(){throw Error('unexpected source boundary error');}};
 const boundary=method(spotFile,'KrakenSpotExchange','fetchBalance',{ENDPOINTS:spotTypes.ENDPOINTS,normalizeAsset:spotTypes.normalizeAsset},receiver);
 const result=await boundary();
 return {spotBalances:cases,spotBalanceBoundary:{raw,keys:Object.keys(result.currencies),units:Object.values(result.currencies).map(c=>Array.from({length:c.info.balance.length},(_,i)=>c.info.balance.charCodeAt(i)))}};
};
