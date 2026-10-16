const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'node_modules/typescript'));
const file=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts'),ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),99,true);
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='HyperliquidExchange');
const names={mode:'resolveUserAbstractionMode',role:'resolveApplicationIdFromKey',refresh:'refreshUnifiedSpotCollateralBalance'};
const code={};for(const [key,name]of Object.entries(names)){const member=cls.members.find(n=>n.name?.getText(ast)===name);assert(member?.initializer,'actual bootstrap initializer '+name);code[key]=ts.transpileModule('return ('+member.initializer.getText(ast)+');',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;}
const mode=['unifiedAccount','portfolioMargin','disabled','default','dexAbstraction','standard','empty','post-error','log-error','timeout','no-user','application-only'];
const role=['user','agent','agent-missing','subAccount','subAccount-missing','vault','missing','unknown','post-error','no-user','existing','existing-vault','swap-options','type-mutates-key','timeout'];
const refresh=['baseline','empty','post-error','handler-error','log-error','disposed','no-track','no-user','throttle','force','concurrent','drop','application-only','sync-throw'];
(async()=>{
 let completed=0,last;
 for(const [family,scenes]of [['mode',mode],['role',role],['refresh',refresh]])for(const scene of scenes){
  const effects=[],channels=[],trace=[],marker={name:'MarkerError',message:'native marker',response:{status:429}},logError={name:'LogMarker',message:'log marker'};
  let posted=0,resolvePost,rejectPost;
  const postPromise=new Promise((resolve,reject)=>{resolvePost=resolve;rejectPost=reject});
  const data=family==='mode'?(scene==='empty'?null:['post-error','log-error','timeout','no-user','application-only'].includes(scene)?'unifiedAccount':scene):family==='role'?{role:scene==='agent-missing'?'agent':scene==='subAccount-missing'?'subAccount':['swap-options','type-mutates-key','timeout'].includes(scene)?'user':scene,data:{user:scene==='agent'?'agent-master':undefined,master:scene==='subAccount'?'sub-master':undefined}}:scene==='empty'?null:{balances:[{coin:'USDC',total:'17'}]};
  const options={key:'key',vault:family==='role'?'':'vault',applicationId:''};if(scene==='no-user'){options.key='';options.vault=''}if(scene==='existing')options.applicationId='existing';if(scene==='application-only'){options.vault='';options.applicationId='app-master'}if(scene==='existing-vault')options.vault='saved-vault';
  const target={options,isDisposed:scene==='disposed',lastSpotCollateralRefreshAt:['throttle','force'].includes(scene)?99999:0,spotCollateralRefreshInFlight:null,
   infoXhr:{post(url,body,config){trace.push('post');posted++;assert.equal(this,target.infoXhr,'source original HTTP receiver');assert.equal(url,'/info','source endpoint');assert.equal(body.type,{mode:'userAbstraction',role:'userRole',refresh:'spotClearinghouseState'}[family],'source native request type');assert.equal(body.user,family==='role'?(scene==='type-mutates-key'?'new-key':'key'):scene==='application-only'?'app-master':'vault','source native fresh request user');assert.equal(config.headers['Content-Type'],'application/json','source request header');if(scene==='sync-throw')throw marker;return postPromise}},
   get withStartupTimeout(){trace.push('timeout-get');return function(operation,ms,name,fallback){assert.equal(this,target,'source timeout receiver');assert.equal(ms,family==='mode'?11:13,'source native timeout duration');assert.equal(name,family==='mode'?'userAbstraction resolution':'userRole resolution','source native timeout name');assert.equal(fallback,family==='mode'?'standard':null,'source native timeout fallback');return scene==='timeout'?Promise.resolve(fallback):operation}},
   log(...args){assert.equal(this,target,'source diagnostic receiver');effects.push(args);channels.push('log');if(scene==='log-error')throw logError},
   emitAndLogError(...args){assert.equal(this,target,'source error receiver');effects.push(args);channels.push('emit')},
   shouldTrackSpotHoldings(){return scene!=='no-track'},shouldUseSpotCollateralBalance(){return scene==='portfolioMargin'},
   handleSpotClearinghouseStateUpdate(value,origin){assert.equal(this,target,'source update receiver');assert.equal(origin,'rest','source update origin');if(data)assert.equal(value,data,'source original collateral object');else assert.deepEqual(value,{balances:[]},'source empty collateral object');effects.push([value,origin]);channels.push('update');if(scene==='handler-error')throw marker}
  };
  const INFO_TYPES={USER_ABSTRACTION:'userAbstraction',SPOT_CLEARINGHOUSE_STATE:'spotClearinghouseState',get USER_ROLE(){if(scene==='type-mutates-key')options.key='new-key';return 'userRole'}};
  const staticFields={USER_ABSTRACTION_RESOLVE_TIMEOUT_MS:11,USER_ROLE_RESOLVE_TIMEOUT_MS:13,SPOT_COLLATERAL_REFRESH_INTERVAL_MS:10000};
  const body=new Function('INFO_TYPES','ENDPOINTS','HyperliquidExchange','LogSeverity','Date',code[family]).call(target,INFO_TYPES,{INFO:'/info'},staticFields,{Info:'info',Debug:'debug',Warning:'warning'},{now:()=>100000});
  let result;const operation=body(['force','concurrent'].includes(scene));operation.then(value=>{result={value}},error=>{result={error}});
  const early=['no-user','existing','existing-vault','no-track','throttle'].includes(scene);
  if(!early){assert.equal(posted,1,'source eager HTTP count');assert.deepEqual(trace,family==='refresh'?['post']:['timeout-get','post'],'source selected timeout before eager fetch')}
  let second;if(scene==='concurrent'){const flight=target.spotCollateralRefreshInFlight;second=body(true);assert.equal(target.spotCollateralRefreshInFlight,flight,'source shared flight identity');assert.notEqual(operation,second,'source distinct outer promises');assert.equal(posted,1,'source concurrent suppression')}
  const replacement={key:'replacement-key'};if(scene==='swap-options')target.options=replacement;
  if(posted&&scene!=='sync-throw'){if(['post-error','log-error'].includes(scene))rejectPost(marker);else resolvePost({data})}
  await operation.catch(()=>{});if(second)await second;
  for(let turn=0;turn<12;turn++)await Promise.resolve();
  if(scene==='log-error')assert.equal(result.error,logError,'source original throwing logger');else assert.equal(result.value,undefined,'source void result');
  assert.equal(posted,early?0:1,'source independent HTTP count');
  if(family==='mode'&&!early&&scene!=='log-error'){
   const expected=scene==='application-only'?'unifiedAccount':['unifiedAccount','portfolioMargin','disabled','default','dexAbstraction'].includes(scene)?scene:'standard';assert.equal(target.userAbstractionMode,expected,'source mode assignment');assert.equal(channels.at(-1),'log','source detected-mode channel');assert.equal(effects.length,['standard','empty','post-error'].includes(scene)?2:1,'source mode diagnostic count');assert.deepEqual(effects.at(-1),['Detected Hyperliquid account abstraction mode: '+expected,scene==='portfolioMargin'?'info':'debug'],'source mode diagnostic');if(['standard','empty'].includes(scene))assert.deepEqual(effects[0],['userAbstraction returned unexpected mode='+(scene==='empty'?'':'standard')+'; defaulting to standard','warning'],'source unexpected-mode warning');
  }
  if(family==='mode'&&['post-error','log-error'].includes(scene)){assert.equal(channels[0],'log','source catch log channel');assert.deepEqual(effects[0],['userAbstraction resolution failed: [object Object]','warning'],'source mode catch warning')}
  if(family==='role'){
   if(!early&&scene!=='timeout'){
    const expected={user:['log','Resolved applicationId from userRole=user (key is master wallet)','info'],'type-mutates-key':['log','Resolved applicationId from userRole=user (key is master wallet)','info'],'swap-options':['log','Resolved applicationId from userRole=user (key is master wallet)','info'],agent:['log','Resolved applicationId from userRole=agent: master=agent-master','info'],subAccount:['log','Resolved applicationId from userRole=subAccount: master=sub-master','info'],vault:['log','Resolved vault from userRole=vault (key is vault wallet)','info'],'agent-missing':['emit','Hyperliquid recognises this wallet as an agent but did not return its master address. Please re-add the account.'],'subAccount-missing':['emit','Hyperliquid recognises this wallet as a subaccount but did not return its master address. Please re-add the account.'],missing:['emit','Hyperliquid does not recognise wallet key. The wallet has no deposits, agent approvals, or subaccount linkage on this network. Please re-add the account using the correct wallet address (and main wallet if this is an agent key).'],'post-error':['log','userRole resolution failed: [object Object]','warning'],unknown:['log','userRole returned unexpected role=unknown; leaving applicationId unresolved','warning']}[scene];
    assert.equal(channels[0],expected[0],'source exact role diagnostic channel');assert.deepEqual(effects[0],expected.slice(1),'source exact role diagnostic args');
   }
   if(scene==='swap-options'){assert.equal(target.options,replacement,'source replacement options');assert.equal(replacement.applicationId,'replacement-key','source fresh post-await key');assert.equal(options.applicationId,'','source original options untouched')}
   else{assert.equal(target.options,options,'source in-place options');const expected={user:'key','type-mutates-key':'new-key',agent:'agent-master',subAccount:'sub-master',existing:'existing'}[scene];assert.equal(options.applicationId,expected??'','source role applicationId');if(scene==='vault')assert.equal(options.vault,'key','source vault routing')}
   assert.equal(effects.length,early||scene==='timeout'?0:1,'source role diagnostic count');
  }
  if(family==='refresh'){
   if(['post-error','handler-error','log-error','sync-throw'].includes(scene)){const index=scene==='handler-error'?1:0;assert.equal(channels[index],'log','source refresh warning channel');assert.deepEqual(effects[index],['Unified spot collateral refresh failed: [object Object]','warning'],'source refresh warning args')}
   if(scene==='sync-throw')assert(target.spotCollateralRefreshInFlight,'source settled flight assignment after synchronous finally');else assert.equal(target.spotCollateralRefreshInFlight,null,'source finally clears flight');assert.equal(target.lastSpotCollateralRefreshAt,early?(scene==='throttle'?99999:0):100000,'source refresh timestamp');assert.equal(effects.length,early||scene==='disposed'?0:scene==='handler-error'?2:1,'source refresh effect count');
  }
  completed++;last=family+'/'+scene;
 }
 assert.equal(completed,41,'independent completed source bootstrap floor');assert.equal(last,'refresh/sync-throw','source bootstrap last case index40');console.log(JSON.stringify({cases:completed,last}));
})().catch(e=>{console.error(e);process.exitCode=1});
