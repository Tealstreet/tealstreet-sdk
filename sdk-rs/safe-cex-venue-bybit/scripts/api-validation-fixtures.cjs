const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..'),modules=new Map();
function load(file){
 if(modules.has(file))return modules.get(file);const exports={};modules.set(file,exports);
 const context={exports,require(spec){
  if(spec.startsWith('.')||spec.startsWith('@tealstreet/')){
   const target=spec.startsWith('.')?path.resolve(path.dirname(file),spec):path.join(root,'packages',spec.slice('@tealstreet/'.length));
   return load(fs.existsSync(target+'.ts')?target+'.ts':path.join(target,'index.ts'));
  }
  return require(require.resolve(spec,{paths:[root,path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'../..')]}));
 }};
 vm.runInNewContext(ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});return exports;
}
const dir=path.join(root,'packages/safe-cex/src/exchanges/bybit'),rates=load(path.join(dir,'bybit.rateLimits.ts')),v=load(path.join(root,'packages/safe-cex/src/utils/get-key.ts')).v;
const file=path.join(dir,'bybit.types.ts'),ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),decs=ast.statements.flatMap(s=>ts.isVariableStatement(s)?Array.from(s.declarationList.declarations):[]).filter(d=>d.name.getText(ast)==='ENDPOINTS');assert.equal(decs.length,1);
const ctx=vm.createContext({});vm.runInContext(ts.transpileModule(`const ${decs[0].getText(ast)};globalThis.endpoints=ENDPOINTS`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,ctx);
function subject(family,names){
 const [file,name]=({linear:['bybit.exchange.ts','BybitExchange'],inverse:['bybitinverse.exchange.ts','BybitInverseExchange'],spot:['bybitspot.exchange.ts','BybitSpotExchange'],hyro:['hyrotrader.exchange.ts','HyrotraderExchange']})[family];
 const ast=ts.createSourceFile(file,fs.readFileSync(path.join(dir,file),'utf8'),ts.ScriptTarget.Latest,true),cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text===name);assert(cls);
 const body=names.map(n=>{const found=cls.members.filter(m=>m.name?.getText(ast)===n);assert.equal(found.length,1,`${name}.${n}`);return found[0].getText(ast);}).join('\n');
 const context=vm.createContext({Error,ENDPOINTS:ctx.endpoints,v,...rates,BYBITINVERSE_RATE_LIMIT_TABLE:rates.BYBIT_RATE_LIMIT_TABLE,BybitInverseAccountType:rates.BybitAccountType,BybitInverseProductType:rates.BybitProductType});
 vm.runInContext(ts.transpileModule(`class Subject {${body}} globalThis.subject=new Subject()`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);return context.subject;
}
function observe(value){
 if(value===undefined)return {undefined:true};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {number:Number.isNaN(value)?'nan':b.toString('hex')};}
 if(Array.isArray(value))return Array.from(value,observe);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
 return value;
}
const error=phase=>({name:`${phase}Error`,message:`${phase} failed`,code:17});
const outcome=(e,errors)=>({error:e.name??null,message:e.message??null,origin:Object.keys(errors).find(k=>errors[k]===e)||null});
async function main(){
 const hyroReceiverAudit = await require("./hyro-account-fixtures.cjs")();
 const validations=[],hyro=[],fetches=[],caches=[],vips=[];
 const rich={name:'RequestError',message:'fallback message',response:{data:{message:'LOG-37',msg:'RESULT-91'}}};
 const inputs=[
 {data:{retMsg:'OK'}},{data:{retMsg:'denied'}},{data:{retMsg:'prefix timestamp and recv_window param suffix'}},
 {data:{retMsg:['timestamp and recv_window param']}},{data:{retMsg:['not timestamp']}},{data:{retMsg:null}},{data:{}},{data:{retMsg:13}},
 {data:{retMsg:'OK'},balanceFail:true},{data:{retMsg:'OK'},balanceSyncFail:true},
 {reject:rich},{reject:{message:'generic'}},{reject:{message:'fallback',response:{data:{message:'',msg:''}}}},{reject:{}},{reject:false},
 {reject:{response:{data:{message:3,msg:['RETURN',9]}},message:'other'}},
 {data:{retMsg:'denied'},failLogs:[1]},{data:{retMsg:'denied'},failLogs:[1,2]},
 {reject:rich,failLogs:[1]},{data:{retMsg:13},failLogs:[2]},
 ];assert.equal(inputs.length,20,'API independent validation input floor');
 for(const family of ['linear','inverse','spot'])for(const input of inputs){
  const api=subject(family,['validateAccount']),trace=[],logs=[],balance={marker:'BALANCE-73'},errors={first:error('first'),second:error('second'),balance:rich},hasReject=Object.hasOwn(input,'reject');let balanceCalls=0;
  api.xhr={get:async endpoint=>{trace.push(['get',endpoint]);await Promise.resolve();trace.push(['resolved']);if(hasReject)throw input.reject;return {data:input.data};}};
  api.emitAndLogError=value=>{trace.push(['log',observe(value)]);logs.push(observe(value));if(input.failLogs?.includes(logs.length))throw logs.length===1?errors.first:errors.second;};
  api.fetchBalance=()=>{balanceCalls++;trace.push(['balance']);if(input.balanceSyncFail)throw errors.balance;return input.balanceFail?Promise.reject(errors.balance):Promise.resolve(balance);};
  let result;try{const value=await api.validateAccount();result={value:observe(value),sameBalance:value.balance===balance};}catch(e){result=outcome(e,errors);}
  validations.push({family,dataRaw:JSON.stringify(input.data||{}),hasReject,rejectRaw:hasReject?JSON.stringify(input.reject):null,balanceFail:input.balanceFail||false,balanceSyncFail:input.balanceSyncFail||false,failLogs:input.failLogs||[],balanceCalls,trace,result});
 }assert.equal(validations.length,60,'API independent validation floor');
 for(const input of [{},{fail:true},{syncFail:true},{fail:true,logFail:true},{fail:true,message:''},{syncFail:true,message:'sync failure'}]){
  const api=subject('hyro',['validateAccount']),trace=[],balance={marker:'HYRO-29'},original=Object.assign(new Error(input.message??'hyro failed'),{name:'BalanceError',code:17}),logging=Object.assign(new Error('log failed'),{name:'LogError'});
  api.fetchBalance=()=>{trace.push(['balance']);if(input.syncFail)throw original;return input.fail?Promise.reject(original):Promise.resolve(balance);};
  api.emitAndLogError=value=>{trace.push(['log',value]);if(input.logFail)throw logging;};
  let result;try{const value=await api.validateAccount();result={value:observe(value),sameBalance:value.balance===balance};}catch(e){result=outcome(e,{original,logging});}
  hyro.push({input,trace,result});
 }assert.equal(hyro.length,6,'API independent Hyro floor');
 const fetchInputs=[
 {data:{retCode:0,result:{vipLevel:'VIP-1',userID:'USER-37'}}},{data:{retCode:'0',result:{vipLevel:'VIP-2'}}},
 {data:{retCode:false,result:{vipLevel:'No VIP'}}},{data:{ret_code:'0',result:{vipLevel:'PRO-2'}}},
 {data:{retMsg:'missing'}},{data:{retCode:1,retMsg:'denied'}},{data:{retCode:null,retMsg:'null code'}},
 {data:{retCode:[],result:{vipLevel:'VIP-3'}}},{data:{retCode:1,retMsg:['denied',7]}},
 {data:{retCode:0,result:null}},{data:{retCode:0}},{data:{retCode:0},phase:'request'},
 {data:{retCode:0,result:{vipLevel:'VIP-1'}},phase:'setter'},{data:{retCode:1,retMsg:'denied'},phase:'log'},
 ];assert.equal(fetchInputs.length,14,'API independent key input floor');
 for(const family of ['linear','inverse'])for(const input of fetchInputs){
  const api=subject(family,['fetchApiKeyInfo']),trace=[],errors=Object.fromEntries(['request','setter','log'].map(k=>[k,error(k)]));let cached={vipLevel:'OLD'},received,sets=0;
  Object.defineProperty(api,'apiKeyInfo',{get:()=>cached,set:value=>{trace.push(['setter',observe(value)]);received=value;sets++;if(input.phase==='setter')throw errors.setter;cached=value;}});
  api.xhr={get:async endpoint=>{trace.push(['get',endpoint]);await Promise.resolve();trace.push(['resolved']);if(input.phase==='request')throw errors.request;return {data:input.data};}};
  api.emitAndLogError=value=>{trace.push(['log',observe(value)]);if(input.phase==='log')throw errors.log;};
  let result;try{const value=await api.fetchApiKeyInfo();result={value:observe(value),sameCache:value===cached,sameReceived:value===received};}catch(e){result=outcome(e,errors);}
  fetches.push({family,input,trace,result,cached:observe(cached),received:observe(received),sets});
 }assert.equal(fetches.length,28,'API independent key floor');
 const cacheInputs=[
 {},{info:null},{info:false},{info:{}},{info:{vipLevel:''}},{info:{vipLevel:'VIP-2'}},
 {phase:'request'},{phase:'requestSync'},{phase:'batch'},{phase:'user'},{phase:'log'},{phase:'fallbackLog'},
 {phase:'log',fallbackFails:true},{replaceOnBatch:{vipLevel:'PRO-3'}},{replaceOnUser:{vipLevel:'VIP-5'}},{missingInfo:true},
 ];assert.equal(cacheInputs.length,16,'API independent cache input floor');
 for(const family of ['linear','inverse'])for(const input of cacheInputs){
  const api=subject(family,['fetchAndCacheApiKeyInfo']),trace=[],errors=Object.fromEntries(['request','batch','user','log','fallback'].map(k=>[k,error(k)]));let cached={vipLevel:'OLD'},batch=7,user,updates=0;
  Object.defineProperty(api,'apiKeyInfo',{get:()=>cached});api.batchLimit=batch;
  api.fetchApiKeyInfo=()=>{trace.push(['fetch']);if(input.phase==='requestSync')throw errors.request;return (async()=>{await Promise.resolve();if(['request','fallbackLog'].includes(input.phase))throw errors.request;cached=input.missingInfo?undefined:Object.hasOwn(input,'info')?input.info:{vipLevel:'VIP-1'};})()};
  api.updateBatchLimitBasedOnVipLevel=()=>{trace.push(['batch']);if(input.phase==='batch')throw errors.batch;api.batchLimit=20;if(input.replaceOnBatch)cached=input.replaceOnBatch;};
  api.store={updateUserData(value){trace.push(['user',observe(value)]);updates++;if(input.phase==='user')throw errors.user;user=value;if(input.replaceOnUser)cached=input.replaceOnUser;}};
  api.log=value=>{trace.push(['log',value]);if(value==='Failed to fetch API key info, continuing without it'){if(input.phase==='fallbackLog'||input.fallbackFails)throw errors.fallback;}else if(input.phase==='log')throw errors.log;};
  let result;try{await api.fetchAndCacheApiKeyInfo();result={ok:true};}catch(e){result=outcome(e,errors);}
  caches.push({family,input,trace,result,cached:observe(cached),batch:observe(api.batchLimit),user:observe(user),updates});
 }assert.equal(caches.length,32,'API independent cache floor');
 const vipInputs=[];
 for(const level of rates.BYBIT_RATE_LIMIT_TABLE.map(row=>row.level))for(const account of ['CLASSIC','UNIFIED'])vipInputs.push({level,account,batch:'7'});
 vipInputs.push({account:'UNIFIED',batch:'7'},{level:'',account:'UNIFIED',batch:'7'},{level:'Unknown',account:'UNIFIED',batch:'7'},{level:'No VIP',account:'unified',batch:'27'},{level:'VIP-1',account:'UNIFIED',batch:'NaN',phase:'log'},{level:'VIP-2',account:'UNIFIED',batch:'7',phase:'setter'},{level:'VIP-3',account:'UNIFIED',batch:'7',phase:'account'},{level:'VIP-1',account:'CLASSIC',batch:'7',replaceOnAccount:'VIP-4'},{level:'VIP-1',account:'CLASSIC',batch:'7',replaceOnSetter:'VIP-5'});
 assert.equal(vipInputs.length,35,'API independent VIP input floor');
 for(const family of ['linear','inverse'])for(const input of vipInputs){
  const api=subject(family,['updateBatchLimitBasedOnVipLevel']),trace=[],errors=Object.fromEntries(['account','setter','log'].map(k=>[k,error(k)]));let level=input.level,batch=Number(input.batch),reads=0,sets=0;
  Object.defineProperty(api,'apiKeyInfo',{get(){reads++;return level===undefined?undefined:{vipLevel:level};}});
  Object.defineProperty(api,'accountType',{get(){trace.push(['account']);if(input.phase==='account')throw errors.account;if(input.replaceOnAccount)level=input.replaceOnAccount;return input.account;}});
  Object.defineProperty(api,'batchLimit',{get:()=>batch,set(value){sets++;trace.push(['setter',observe(value)]);if(input.phase==='setter')throw errors.setter;batch=value;if(input.replaceOnSetter)level=input.replaceOnSetter;}});
  api.log=value=>{trace.push(['log',value]);if(input.phase==='log')throw errors.log;};
  let result;try{api.updateBatchLimitBasedOnVipLevel();result={ok:true};}catch(e){result=outcome(e,errors);}
  vips.push({family,input,trace,result,reads,sets,level:observe(level),batch:observe(batch)});
 }assert.equal(vips.length,70,'API independent VIP floor');
 process.stdout.write(JSON.stringify({hyroReceiverAudit,validations,hyro,fetches,caches,vips}));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
