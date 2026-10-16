'use strict';
const defaults={testnet:false,applicationId:'APP',key:{opaque:'key'},secret:{opaque:'secret'},topics:{'10':'TEN','2':['TWO',['NESTED']],tail:'TAIL'},data:'{"topic":"tickersSuffix","v":31.5}',parsed:{data:{v:31.5}},handlers:[['tickers',['a','b']],['tickersSuffix',['c']]]};
const cases={};
for(const side of ['public','private'])for(const method of ['connectAndSubscribe','onOpen','subscribe','ping']){
 const key=side+'.'+method;
 cases[key]=[{},{disposed:true},{testnet:true,reconnect:true,applicationId:-0},{fail:method==='connectAndSubscribe'?'listener:message':method==='onOpen'?(side==='public'?'send:2':'sign'):method==='subscribe'?'send:2':'send:1'}];
}
cases['public.connectAndSubscribe'].push({applicationId:{opaque:'application'},fail:'create'});
cases['private.connectAndSubscribe'].push({applicationId:null},{applicationIdMissing:true});
cases['public.onOpen'].push({reconnect:true,mutateDisposed:true},{reconnect:true,fail:'reconnected'});
cases['private.onOpen'].push({reconnect:true},{reconnect:true,fail:'snapshot'});
cases['public.subscribe'].push({topics:{first:[],second:[null,false,0,{opaque:'topic'}]}},{fail:'encode'});
cases['private.subscribe'].push({mutateDisposed:true});
cases['public.ping'].push({clock:NaN},{clock:-0});
cases['private.ping'].push({clock:NaN},{clock:-0});
cases['public.handlePingEvent']=[{},{disposed:true},{fail:'send:1'},{fail:'encode'}];
cases['public.onMessage']=[{},{disposed:true},{data:'unmatched'},{data:'{"topic": "tickers"}'},{data:'{"event":"tickersSuffix"}',append:true},{parsed:null},{parsed:false},{fail:'parse'},{fail:'callback:a'},{mutateHandlers:true},{data:'{"topic":"tickers","event":"other"}',handlers:[['other',['x']],['tickers',['a']]]},{handlers:[['tickers',[]],['tickersSuffix',['c']]]}];
module.exports={defaults,cases};
