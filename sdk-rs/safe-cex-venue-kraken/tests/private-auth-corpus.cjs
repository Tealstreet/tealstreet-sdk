const spotWsFile=path.join(src,'exchanges/kraken/krakenspot.ws-private.ts');
const futuresWsFile=path.join(src,'exchanges/kraken/kraken.ws-private.ts');
const LogSeverity=load(path.join(root,'packages/logging/src/LogSeverity.ts')).LogSeverity;
const tokenTags=['undefined','null','false','true','zero','negative-zero','nan','infinity','negative-infinity','empty','text','array','object'];
function authToken(tag){
 return {undefined:undefined,null:null,false:false,true:true,zero:0,'negative-zero':-0,nan:NaN,infinity:Infinity,'negative-infinity':-Infinity,empty:'',text:'private-雪',array:[],object:{value:-0,other:17.25}}[tag];
}
function authError(config){const e=new Error(config.emptyError?'':'callback failure');e.name='TransportError';return e;}
const spotState=r=>({authenticated:r.authenticated,token:capture(r.wsToken)});
async function spotOpen(config){
 const trace=[];const original=authError(config);let disposed=!!config.disposed;let release;
 const r={authenticated:true,wsToken:'old-token',get isDisposed(){return disposed},parent:{
  fetchWsToken(){trace.push({kind:'fetch',state:spotState(r)});if(config.fetchSync)throw original;return new Promise((resolve,reject)=>{release=()=>config.fetchFail?reject(original):resolve(authToken(config.token))})},
  log(message,severity){trace.push({kind:'log',message,severity,state:spotState(r)});if(config.logFail)throw original},
 },send(wire){trace.push({kind:'send',wire,state:spotState(r)});const count=trace.filter(x=>x.kind==='send').length;
  if(count===1&&config.retoken)r.wsToken=authToken(config.retoken);
  if(count===1&&config.disposeSend)disposed=true;
  if(count===config.sendFail)throw original;
 }};
 const fn=method(spotWsFile,'KrakenSpotPrivateWebsocket','onOpen',{LogSeverity},r);
 const promise=fn();const prefix=spotState(r);const prefixTrace=trace.length;
 if(config.disposeAwait)disposed=true;
 if(config.setAwait)r.wsToken='during-await';
 if(release)release();
 let failure=null;try{await promise}catch(e){failure={name:e.name,message:e.message,identity:e===original}}
 return {config,prefix,prefixTrace,state:spotState(r),trace,failure};
}
async function futuresAuth(config){
 const trace=[];const original=authError(config);const r={waitingChallenge:!!config.waiting,parent:{options:{key:config.key,secret:config.secret,remoteSigner:config.remote?()=>{}:undefined},
  log(message,severity){trace.push({kind:'log',message,severity,waiting:r.waitingChallenge});if(config.logFail)throw original},
 },send(wire){trace.push({kind:'send',wire,waiting:r.waitingChallenge});if(config.resetSend)r.waitingChallenge=false;if(config.sendFail)throw original}};
 const fn=method(futuresWsFile,'KrakenPrivateWebsocket','authenticate',{LogSeverity},r);
 let failure=null;try{await fn()}catch(e){failure={name:e.name,message:e.message,identity:e===original}}
 return {config,waiting:r.waitingChallenge,trace,failure};
}
async function futuresOpen(config){
 const trace=[];const original=authError(config);let disposed=!!config.disposed;let release;
 const r={get isDisposed(){return disposed},authenticate(){trace.push({kind:'auth'});if(config.syncFail)throw original;return new Promise((resolve,reject)=>{release=()=>config.fail?reject(original):resolve()})},parent:{
  log(message,severity){trace.push({kind:'log',message,severity});if(config.logFail)throw original}
 }};
 const fn=method(futuresWsFile,'KrakenPrivateWebsocket','onOpen',{LogSeverity},r);
 const promise=fn();const prefixTrace=trace.length;disposed=true;if(release)release();
 let failure=null;try{await promise}catch(e){failure={name:e.name,message:e.message,identity:e===original}}
 return {config,prefixTrace,trace,failure};
}
async function privateAuthCorpus(){
 const setters=tokenTags.map(token=>{const r={authenticated:true,wsToken:'old-token'};method(spotWsFile,'KrakenSpotPrivateWebsocket','setToken',{},r)(authToken(token));return {token,state:spotState(r)}});
 const spot=[];for(const token of tokenTags)spot.push(await spotOpen({token}));
 for(const config of [{disposed:true},{token:'text',disposeAwait:true,setAwait:true},{fetchSync:true},{fetchFail:true},{token:'text',sendFail:1},{token:'text',sendFail:2},{token:'text',retoken:'undefined'},{token:'text',retoken:'object'},{token:'text',disposeSend:true},{fetchFail:true,logFail:true},{token:'empty',logFail:true}])spot.push(await spotOpen(config));
 const auth=[];for(const config of [{},{key:''},{key:'K'},{key:'K',secret:''},{key:'K',secret:'S'},{key:'K',remote:true},{key:'K',secret:'S',sendFail:true},{key:'K',secret:'S',sendFail:true,emptyError:true},{key:'K',secret:'S',sendFail:true,logFail:true},{waiting:true,logFail:true},{key:'K',secret:'S',resetSend:true}])auth.push(await futuresAuth(config));
 const open=[];for(const config of [{disposed:true},{},{fail:true},{syncFail:true},{fail:true,emptyError:true},{fail:true,logFail:true}])open.push(await futuresOpen(config));
 return {setters,spot,auth,open};
}
