'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const src = path.join(root, 'packages/safe-cex/src');
const cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file);
  assert.ok(file.startsWith(src + path.sep) || file === path.join(root, 'packages/logging/src/LogSeverity.ts') || file === path.join(root,'packages/math/src/isNotNumber.ts'), `source outside safe-cex: ${file}`);
  const source = fs.readFileSync(file, 'utf8');
  const exports = {};
  cache.set(file, exports);
  const result = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
    reportDiagnostics: true,
  });
  assert.equal((result.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  vm.runInNewContext(result.outputText, { exports, require(name) {
    if (name === '@tealstreet/logging/src/LogSeverity') return load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
    if(name==='@tealstreet/math/src/isNotNumber')return load(path.join(root,'packages/math/src/isNotNumber.ts'));
    assert.ok(name.startsWith('.'), `unexpected runtime dependency ${name}`);
    return load(path.resolve(path.dirname(file), name) + '.ts');
  } }, { filename: file });
  return exports;
}

const crypto=require('node:crypto');
const hashes={};
function method(file,className,name,bindings,receiver) {
 const source=fs.readFileSync(file,'utf8');
 hashes[path.basename(file)]=crypto.createHash('sha256').update(source).digest('hex');
 const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
 const classes=ast.statements.filter(s=>ts.isClassDeclaration(s)&&s.name.text===className);
 assert.equal(classes.length,1,'actual class selected');
 const members=classes[0].members.filter(m=>m.name?.getText(ast)===name);
 assert.equal(members.length,1,`actual method ${name}`);
 assert.ok(ts.isPropertyDeclaration(members[0])&&ts.isArrowFunction(members[0].initializer),'actual arrow body selected');
 const result=ts.transpileModule(`function make(){return (${members[0].initializer.getText(ast)});}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
 assert.equal((result.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
 return vm.runInNewContext(result.outputText+';make',bindings,{filename:file}).call(receiver);
}
function capture(v){
 if(v===undefined)return {undefined:true};
 if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {number:Number.isNaN(v)?'7ff8000000000000':b.toString('hex')};}
 if(Array.isArray(v))return v.map(capture);
 if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,capture(x)]));
 return v;
}
const value=raw=>raw===null?undefined:JSON.parse(raw);
const futuresFile=path.join(src,'exchanges/kraken/kraken.exchange.ts');
const spotFile=path.join(src,'exchanges/kraken/krakenspot.exchange.ts');
const types=load(path.join(src,'types.ts'));
const futuresTypes=load(path.join(src,'exchanges/kraken/kraken.types.ts'));
const spotTypes=load(path.join(src,'exchanges/kraken/krakenspot.types.ts'));
const income=load(path.join(src,'exchanges/kraken/kraken.income.ts'));
const getKeyFile=path.join(src,'utils/get-key.ts'),getKeyText=fs.readFileSync(getKeyFile,'utf8');hashes[path.basename(getKeyFile)]=crypto.createHash('sha256').update(getKeyText).digest('hex');
const getKeyAst=ts.createSourceFile(getKeyFile,getKeyText,ts.ScriptTarget.Latest,true),decl=getKeyAst.statements.filter(ts.isVariableStatement).flatMap(statement=>[...statement.declarationList.declarations]).find(declaration=>declaration.name.getText(getKeyAst)==='v');assert.ok(decl);
const requireLodash=name=>require(path.join(root,'node_modules/lodash',name));
const getV=vm.runInNewContext(ts.transpileModule('function make(){return ('+decl.initializer.getText(getKeyAst)+')}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText+';make()', {s:requireLodash('memoize')(requireLodash('snakeCase')),c:requireLodash('memoize')(requireLodash('camelCase'))});
const precision=load(path.join(src,'utils/ccxt_helpers/precision.ts'));
const fills=load(path.join(src,'exchanges/kraken/kraken.fills.ts'));
const actualAdjust=load(path.join(src,'utils/safe-math.ts')).adjust;
const configs=require('./trading-fixtures.cjs');
const triggerText=fs.readFileSync(futuresFile,'utf8'),triggerAst=ts.createSourceFile(futuresFile,triggerText,ts.ScriptTarget.Latest,true);
const triggerDecl=triggerAst.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).filter(d=>d.name.getText(triggerAst)==='parseTriggerSignal');assert.equal(triggerDecl.length,1);
const parseSignal=vm.runInNewContext(ts.transpileModule('('+triggerDecl[0].initializer.getText(triggerAst)+')',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,{OrderStopTriggerType:types.OrderStopTriggerType});
function err(e,original,response){return {identity:e===original,name:e.name,message:e.message,responseIdentity:e.response===response,causeIdentity:e.cause===original};}
function apiDeclaration(file,bindings){
 const source=fs.readFileSync(file,'utf8');hashes[path.basename(file)]=crypto.createHash('sha256').update(source).digest('hex');
 const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
 const names=new Set(['sort','nonceCounter','createKrakenApiError','createAPI']);const picked=ast.statements.filter(s=>ts.isVariableStatement(s)&&[...s.declarationList.declarations].some(d=>names.has(d.name.getText(ast))));
 assert.equal(picked.filter(s=>[...s.declarationList.declarations].some(d=>d.name.getText(ast)==='createAPI')).length,1);
 const code='function make(){'+picked.map(s=>s.getText(ast).replace(/^export\s+/, '')).join('\n')+';return {createAPI'+(file.endsWith('/kraken.api.ts')?',counter:()=>nonceCounter,setCounter:n=>nonceCounter=n':'')+'};}';
 return vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText+';make()',bindings,{filename:file});
}
(async()=>{
 const trading={},api={};
 for(const [family,cases] of Object.entries(configs.trading)){
  trading[family]=[];
  for(const raw of cases){
   const cfg=structuredClone(raw);const events=[];const original=new Error(cfg.errorMessage??'read failure');if(cfg.errorResponse)original.response=cfg.errorResponse;
   const store=cfg.store;const margin=new Map(Object.entries(cfg.margin||{})),leverage=new Map(Object.entries(cfg.leverage||{}));let release,reject,calls=0;
   const wait=(kind,argument)=>{events.push({kind,argument});const n=calls++;const result=cfg[kind+'Result']??[];if(n===0)return new Promise((r,j)=>{release=()=>cfg[kind+'Fail']?j(original):r(result)});return cfg[kind+'Fail']?Promise.reject(original):Promise.resolve(result);};
   const receiver={store,marginTypePreferences:margin,leverageSettings:leverage,
    resolveCallerClientOrderId(o){events.push({kind:'resolve',identity:o===cfg.input});if(cfg.resolveFail)throw original;},
    deriveAlgoOrdersFromNormalOrdersOpts(o){events.push({kind:'derive',identity:o[0]===cfg.input});if(cfg.deriveFail)throw original;return 'derived';},
    placeOrders:o=>wait('placeOrders',o),placeOrder:o=>wait('placeOrder',o),cancelOrders:o=>wait('cancelOrders',o),fetchPositions:()=>wait('fetchPositions',undefined),
    emitAndLogError(message,context){events.push({kind:'emit',message,context});if(cfg.emitFail)throw original;},
    log(message,severity){events.push({kind:'log',message,severity});if(cfg.logFail)throw original;}
   };
   store.addOrUpdatePositions=positions=>{events.push({kind:'addPositions',positions});if(cfg.addFail)throw original;};
   const send=(spot,client,method)=>(endpoint,body)=>{events.push({kind:'send',spot,client,method,endpoint,body});if(cfg.syncFail)throw original;const n=calls++;if(n===0)return new Promise((r,j)=>{release=()=>cfg.fail?j(original):r({data:cfg.data})});return cfg.fail?Promise.reject(original):Promise.resolve({data:cfg.data});};
   const spot=family.startsWith('spot');receiver.xhr={put:send(spot,'xhr','put')};receiver.unlimitedXHR={post:send(spot,'unlimited','post')};
   const bindings={...types,ENDPOINTS:spot?spotTypes.ENDPOINTS:futuresTypes.ENDPOINTS,ORDER_TYPE:spotTypes.ORDER_TYPE,LogSeverity:load(path.join(root,'packages/logging/src/LogSeverity.ts')).LogSeverity,
    adjust(value,step){events.push({kind:'adjust',amount:value,precision:step});if(cfg.adjustFail)throw original;return actualAdjust(value,step);},
    uuid(){events.push({kind:'uuid'});return 'uuid-A';},createKrakenSpotClientOrderId(){events.push({kind:'spotId'});return cfg.clientId??'0123456789abcdef0123456789abcdef';},KRAKEN_SPOT_CL_ORD_ID_PATTERN:/^[a-f0-9]{32}$/,
    parseTriggerSignal:parseSignal,v:getV,KRAKEN_EDIT_SUCCESS_STATUSES:new Set(['edited','filled']),KRAKEN_CANCEL_SUCCESS_STATUSES:new Set(['cancelled'])
   };
   const name={futuresPlace:'placeOrder',spotPlace:'placeOrder',futuresUpdate:'updateOrder',spotUpdate:'updateOrder',futuresCancel:'cancelOrders',leverage:'setLeverage',margin:'changeMarginType'}[family];
   const fn=method(spot?spotFile:futuresFile,spot?'KrakenSpotExchange':'KrakenExchange',name,bindings,receiver);
   let promise;if(family==='leverage')promise=fn(cfg.input.symbol,cfg.input.leverage);else if(family==='margin')promise=fn(cfg.input.isolated,cfg.input.symbol);else promise=fn(cfg.input);
   const prefix=events.length;
   Object.assign(store,cfg.after||{});Object.assign(cfg.input,cfg.afterInputs||{});if(cfg.afterOrder)Object.assign(cfg.input.order,cfg.afterOrder);
   if(release)release();let result,error;
   try{result=await promise;}catch(e){error=err(e,original);}
   trading[family].push({cfg:capture(raw),prefix,result:capture(result),error,events:capture(events),margin:capture(Object.fromEntries(margin)),leverage:capture(Object.fromEntries(leverage))});
  }
 }
 for(const [family,cases] of Object.entries(configs.api)){
  api[family]=[];
  for(const raw of cases){
   const cfg=structuredClone(raw);const events=[];const original=new Error(cfg.errorMessage??'read failure');if(cfg.errorResponse)original.response=cfg.errorResponse;
   const clients=[];
   const spot=family==='spotApi';const constants=spot?spotTypes:futuresTypes;
   const bindings={...constants,Buffer:{from(value,encoding){if(encoding==='base64'){events.push({kind:'secret',value});return 'SECRET';}return value;},concat(parts){events.push({kind:'pathHash',path:parts[0],hash:parts[1]});return 'PATH_HASH';}},
    safeCexAxiosCreate(config,options,exchange){events.push({kind:'create',config,optionsIdentity:options===cfg.options,exchangeIdentity:exchange===cfg.exchange});if(cfg.createFail)throw original;const c={id:'client',interceptors:{request:{use(fn){events.push({kind:'requestUse'});c.request=fn;}},response:{use(success,rejected){events.push({kind:'responseUse'});c.success=success;c.rejected=rejected;}}}};clients.push(c);return c;},
    retry(client,config){events.push({kind:'retry',config});if(cfg.retryFail)throw original;},
    networkErrorOnRead(posted){events.push({kind:'network',posted});return 'condition';},readsAndPosted(...posted){return posted;},
    installTransportAdapter(client,options,name){events.push({kind:'transport',name});},markSkipRemoteSigning(config){events.push({kind:'skip'});},
    virtualClock:{getCurrentTimeMs:()=>cfg.clock??1723638896789},
    qs:{stringify(value,options){events.push({kind:'qs',value,form:options.format==='RFC1738'&&typeof options.sort==='function'?'sorted1738':'repeat'});if(cfg.qsFail)throw original;return 'qs-result';}},
    JSON:{stringify(value){events.push({kind:'json',value});return 'JSON';}},encodeURIComponent(value){events.push({kind:'encode',value});return 'ENCODED';},
    createHash(algorithm){assert.equal(algorithm,'sha256');return {update(value,encoding){assert.equal(encoding,'utf8');events.push({kind:'sha256',value});if(cfg.hashFail)throw original;return this;},digest(){return 'HASH';}};},
    createHmac(algorithm,secret){assert.equal(algorithm,'sha512');return {update(payload){events.push({kind:'hmac512',secret,payload});return this;},digest(encoding){assert.equal(encoding,'base64');return 'SIGNATURE';}};},v:getV
   };
   const file=path.join(src,'exchanges/kraken',spot?'krakenspot.api.ts':'kraken.api.ts');const module=apiDeclaration(file,bindings);if(!spot&&cfg.counter!==undefined)module.setCounter(cfg.counter);
   let error;try{for(let i=0;i<cfg.clients;i++)module.createAPI(cfg.options,cfg.exchange,cfg.baseKind);}catch(e){error=err(e,original);}
   const steps=[];
   if(!error)for(const step of cfg.steps){const input=step.kind==='rejected'?(cfg.errorResponse??step.input):step.input;Object.assign(cfg.options,step.options||{});const client=step.client??0;let result,error;
    const beforeInput=structuredClone(input);
    try{result=await clients[client][step.kind](step.kind==='rejected'?original:input);}catch(e){error=err(e,original,input);}
    steps.push({kind:step.kind,client,input:capture(beforeInput),after:capture(input),result:capture(result),error,identity:result===input,counter:capture(spot?undefined:module.counter()),options:capture(step.options),qsCalls:events.filter(e=>e.kind==='qs').length-steps.reduce((n,s)=>n+s.qsCalls,0)});
   }
   api[family].push({cfg:capture(raw),error,steps,events:capture(events)});
  }
 }
 process.stdout.write(JSON.stringify({header:{hashes,tz:process.env.TZ},trading,api}));
})().catch(e=>{console.error(e);process.exitCode=1});
