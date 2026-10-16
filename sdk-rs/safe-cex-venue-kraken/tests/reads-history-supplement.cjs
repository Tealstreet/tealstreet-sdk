globalThis.historySupplement = async () => {
 const cases = [];
 const inputs = [
  {account:'account-A',from:17.25,to:93.5},
  {account:'account-B',from:-0,to:0},
  {account:'account-C'},
  {account:'account-D',to:123.75},
  {account:'account-E',from:-27.5},
  {account:'account-F',from:null,to:null},
 ];
 for (let index=0;index<10;index++) {
  const raw=index%6===1?'{"account":"account-B","from":-0,"to":0}':JSON.stringify(inputs[index%6]);
  const opts=JSON.parse(raw), calls=[];
  const inert=index===8, fail=index===9;
  const original=new Error('folder failure');
  const records=[{id:'record-A',symbol:'PF_XBTUSD'},{id:'record-B',symbol:'PF_ETHUSD'}];
  const receiver={constructor:{name:'KrakenExchange'},store:{marketsBySymbol:{PF_XBTUSD:{quote:'USD'},EMPTY:{quote:''}}}};
  receiver.foldClosedPositionRecords=method(path.join(src,'exchanges/base.ts'),'BaseExchange','foldClosedPositionRecords',{
   isInertCapability:()=>inert, Date:{now:()=>901.25},
   buildFoldedPositionRecords:async (venue,options)=>{
    calls.push({account:options.account,from:options.from,to:options.to,symbolScoped:options.symbolScoped,
     sameVenue:venue===receiver,settles:['PF_XBTUSD','EMPTY','MISSING'].map(options.resolveSettleCurrency)});
    if(fail)throw original;
    return {records};
   }
  },receiver);
  const fn=method(futuresFile,'KrakenExchange','fetchHistoricalPositions',{},receiver);
  try { const result=await fn(opts); cases.push({raw,inert,fail,calls:capture(calls),result:capture(result),sameResult:result===records}); }
  catch(error){ cases.push({raw,inert,fail,calls:capture(calls),error:error.message,sameError:error===original}); }
 }
 const raw='{"account":17,"from":29.5,"to":31.75}';
 const boundary=method(futuresFile,'KrakenExchange','fetchHistoricalPositions',{}, {foldClosedPositionRecords:(opts,config)=>({account:opts.account,symbolScoped:config.symbolScoped})});
 return {histories:cases,historyBoundary:{raw,result:capture(await boundary(JSON.parse(raw)))}};
};
