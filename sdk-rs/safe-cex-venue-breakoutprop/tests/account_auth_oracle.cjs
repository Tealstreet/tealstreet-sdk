const {BreakoutpropExchange,store,encode}=require('./portfolio_oracle.cjs');
const account=(name,status='FULL_TRADING',n=1)=>({owner:'owner'+n,account:name,version:n+.125,baseCurrency:'USD',registrationTime:'date'+n,accountStatus:status,pricingStream:'stream'+n,positionBased:n%2===0});
const data=()=>({userDetails:[{login:'login',domain:'default',version:7.125,fullName:'full name',username:'username',accounts:[account('disabled','DISABLED'),account('default:321',undefined,2)]}]});
const names=['single','multi-first','none-valid','no-accounts','no-users','missing-unused','missing-later-user','cached','cached-empty','no-subaccount','empty-subaccount','not-found','session','http','logger','object-msg','empty-msg','surrogate'];
(async()=>{
 const cases=[];
 for(const name of names){
  const input=data(),options={key:'user-A',subaccount:'32'};let cache=null;
  if(['multi-first','no-subaccount','empty-subaccount','not-found'].includes(name))input.userDetails[0].accounts.push(account('other:321',undefined,3));
  if(name==='no-subaccount')delete options.subaccount;if(name==='empty-subaccount')options.subaccount='';if(name==='not-found')options.subaccount='absent';
  if(name==='none-valid')input.userDetails[0].accounts.forEach(a=>a.accountStatus='DISABLED');if(name==='no-accounts')input.userDetails[0].accounts=[];if(name==='no-users')input.userDetails=[];
  if(name==='missing-unused')delete input.userDetails[0].fullName;if(name==='missing-later-user')input.userDetails.push({accounts:[]});
  if(name==='cached')cache='cached-account';if(name==='cached-empty')cache='';if(name==='surrogate')input.userDetails[0].fullName='\ud800';
  const ex=new BreakoutpropExchange('breakoutprop',options,store()),trace=[],errors=[];ex.accountCode=cache;
  const original=Object.assign(Error('original-rejection'),{response:{data:{msg:name==='object-msg'?{detail:'object'}:name==='empty-msg'?'':'venue-message'}}}),logger=Error('logger-rejection');
  ex.getSessionToken=()=>{trace.push('session');return name==='session'?Promise.reject(original):Promise.resolve('token');};
  ex.xhr={get:async(url,config)=>{trace.push({method:'GET',url,params:config.params});if(['http','logger','object-msg','empty-msg'].includes(name))throw original;return {data:input};}};
  ex.emitAndLogError=e=>{errors.push(e);if(name==='logger')throw logger;};
  let expected;try{expected={ok:await ex.getAccountCode(),errors};}catch(e){expected={error:e.message,name:e.name,sameError:e===(name==='logger'?logger:original),errors};}
  cases.push({name,inputAccounts:input.userDetails.map(u=>u.accounts.map(a=>a.account)),raw:encode(input),options,cache,expected,trace,after:ex.accountCode});
 }
 const validations=[];
 for(const name of ['success','message','both','empty','falsy','object','logger','array']){
  const ex=new BreakoutpropExchange('breakoutprop',{},store()),errors=[],trace=[];
  const original=Object.assign(Error(name==='empty'?'':'original-rejection'),{response:{data:{}}}),logger=Error('logger-rejection');
  if(name==='both')original.response.data={message:'log-message',msg:'return-message'};
  if(name==='falsy')original.response.data={message:0,msg:false};if(name==='object')original.response.data={message:{log:'object'},msg:{return:'object'}};if(name==='array')original.response.data={message:[],msg:[]};
  ex.getSessionToken=()=>{trace.push('session');return name==='success'?Promise.resolve('token'):Promise.reject(original);};ex.emitAndLogError=e=>{errors.push(e);if(name==='logger')throw logger;};
  let expected;try{expected={ok:await ex.validateAccount(),errors};}catch(e){expected={error:e.message,sameError:e===logger,errors};}
  validations.push({name,message:original.message,response:original.response,expected,trace});
 }
 const invalidations=[];
 for(const token of ['token',null,'']){const ex=new BreakoutpropExchange('breakoutprop',{},store()),pending=Promise.resolve('pending');ex.sessionToken=token;ex.sessionTokenExpiresAt=9123;ex.sessionTokenRequest=pending;ex.invalidateSession();invalidations.push({before:token,token:ex.sessionToken,expires:ex.sessionTokenExpiresAt,pendingRetained:ex.sessionTokenRequest===pending});}
 const ex=new BreakoutpropExchange('breakoutprop',{key:'ORIGINAL',subaccount:'321'},store());let settle,posted,calls=0;
 ex.getSessionToken=()=>new Promise(r=>{settle=r;});ex.xhr={get:async(_,config)=>{posted=config.params.username;calls++;return {data:data()};}};
 const p=ex.getAccountCode().then(()=>{calls++;});ex.options.key='LATE';settle('');void p;await new Promise(setImmediate);
 const aliasEx=new BreakoutpropExchange('breakoutprop',{},store()),msg={value:'before'};aliasEx.getSessionToken=async()=>{throw Object.assign(Error('alias'),{response:{data:{msg}}});};const aliasResult=await aliasEx.validateAccount();msg.value='after';
 const discardedEx=new BreakoutpropExchange('breakoutprop',{},store());let rejectValidation,validationCalls=0;
 discardedEx.getSessionToken=()=>new Promise((_,r)=>{rejectValidation=r;});discardedEx.emitAndLogError=()=>{validationCalls++;};void discardedEx.validateAccount().then(()=>{validationCalls++;});rejectValidation(Error('discarded'));await new Promise(setImmediate);
 const loggerEx=new BreakoutpropExchange('breakoutprop',{},store()),shared={value:'before'};
 loggerEx.getSessionToken=async()=>{throw Object.assign(Error('alias'),{response:{data:{message:shared,msg:shared}}});};loggerEx.emitAndLogError=m=>{m.value='logged';};const loggerMutation=await loggerEx.validateAccount();
 const consumed=[];
 const minimal=()=>({userDetails:[{accounts:[{account:'default:A',accountStatus:'FULL_TRADING'}]}]});
 for(const name of ['minimal','poisoned-unused','ignored-later','ignored-disabled','arraylike-users','null-user','null-accounts','scalar-accounts','null-account','nonstring-status','missing-status','bad-code-before-second-match']){
  const input=minimal();
  if(name==='poisoned-unused'){
   Object.assign(input.userDetails[0],{login:null,domain:[],version:{},fullName:false,username:12});
   Object.assign(input.userDetails[0].accounts[0],{owner:[],version:null,baseCurrency:{},registrationTime:false,pricingStream:12,positionBased:'unused'});
  }
  if(name==='ignored-later')input.userDetails.push(null,{accounts:'ignored'});
  if(name==='ignored-disabled')input.userDetails[0].accounts.unshift({accountStatus:'DISABLED'});
  if(name==='arraylike-users')input.userDetails={'0':input.userDetails[0]};
  if(name==='null-user')input.userDetails=[null];
  if(name==='null-accounts')input.userDetails[0].accounts=null;
  if(name==='scalar-accounts')input.userDetails[0].accounts='unused';
  if(name==='null-account')input.userDetails[0].accounts=[null];
  if(name==='nonstring-status')input.userDetails[0].accounts[0].accountStatus=['FULL_TRADING'];
  if(name==='missing-status')delete input.userDetails[0].accounts[0].accountStatus;
  if(name==='bad-code-before-second-match')input.userDetails[0].accounts.unshift({accountStatus:'FULL_TRADING'});
  const ex=new BreakoutpropExchange('breakoutprop',{key:'k',subaccount:'A'},store()),errors=[];
  ex.getSessionToken=async()=>'';ex.xhr={get:async()=>({data:input})};ex.emitAndLogError=e=>errors.push(e);
  let expected;try{expected={ok:await ex.getAccountCode(),errors};}catch(e){expected={error:e.message,name:e.name,errors};}
  consumed.push({name,raw:encode(input),expected,after:ex.accountCode??null});
 }
 process.stdout.write(JSON.stringify({runtime:{node:process.version,nodeMajor:+process.versions.node.split('.')[0],arch:process.arch,v8:process.versions.v8},consumed,cases,validations,invalidations,owned:{posted,calls},alias:aliasResult,validationCalls,loggerMutation}));
})().catch(e=>{console.error(e);process.exitCode=1;});
