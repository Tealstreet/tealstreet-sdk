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
  assert.ok(file.startsWith(src + path.sep) || file === path.join(root, 'packages/logging/src/LogSeverity.ts'), `source outside safe-cex: ${file}`);
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
const futuresFile=path.join(src,'exchanges/kraken/kraken.ws-private.ts');
const spotFile=path.join(src,'exchanges/kraken/krakenspot.ws-private.ts');
const types=load(path.join(src,'types.ts'));
const futuresTypes=load(path.join(src,'exchanges/kraken/kraken.types.ts'));
const spotTypes=load(path.join(src,'exchanges/kraken/krakenspot.types.ts'));
const getKeyFile=path.join(src,'utils/get-key.ts'),getKeyText=fs.readFileSync(getKeyFile,'utf8');hashes[path.basename(getKeyFile)]=crypto.createHash('sha256').update(getKeyText).digest('hex');
const getKeyAst=ts.createSourceFile(getKeyFile,getKeyText,ts.ScriptTarget.Latest,true),decl=getKeyAst.statements.filter(ts.isVariableStatement).flatMap(statement=>[...statement.declarationList.declarations]).find(declaration=>declaration.name.getText(getKeyAst)==='v');assert.ok(decl);
const requireLodash=name=>require(path.join(root,'node_modules/lodash',name));
const getV=vm.runInNewContext(ts.transpileModule('function make(){return ('+decl.initializer.getText(getKeyAst)+')}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText+';make()', {s:requireLodash('memoize')(requireLodash('snakeCase')),c:requireLodash('memoize')(requireLodash('camelCase'))});
const createPositionId=load(path.join(src,'utils/createPositionId.ts')).createPositionId;
const configs=require('./private-accounts-fixtures.cjs');
function contains(root,target){if(root===target)return true;return root&&typeof root==='object'&&Object.values(root).some(v=>contains(v,target));}
(async()=>{
 const rows={};
 for(const [family,cases] of Object.entries(configs)){
  rows[family]=[];
  for(const raw of cases){
   const cfg=structuredClone(raw),events=[],store=cfg.store,original=new Error(cfg.errorMessage??'read failure');
   const effect=(kind,details={})=>{events.push({kind,...details});if(cfg.effectFail===kind)throw original; if(events.length===1)Object.assign(store,cfg.afterEffect||{});};
   const orders=()=>store.orders;
   store.updateOrder=(selector,patch)=>{effect('updateOrder',{selector,patch});const o=orders().find(o=>o.id===selector.id);if(o)Object.assign(o,patch);};
   store.updateOrderStatus=(selector,status)=>{effect('status',{selector,status});const o=orders().find(o=>o.id===selector.id);if(o)o.status=status;};
   store.addOrUpdateOrder=order=>{effect('addOrder',{order,infoIdentity:contains(cfg.input,order.info)});const i=orders().findIndex(o=>o.id===order.id);if(i<0)store.orders.push(order);else store.orders[i]=order;};
   store.addOrUpdateOrdersFull=orders=>{effect('fullOrders',{orders,infoIdentity:orders.every(o=>contains(cfg.input,o.info))});store.orders=orders;};
   store.addOrUpdatePositionsFull=positions=>{effect('fullPositions',{positions,infoIdentity:positions.every(p=>contains(cfg.input,p.info))});store.positions=positions;};
   store.update=patch=>effect('update',{patch,infoIdentity:contains(cfg.input,patch.balance.info)});
   const margin=cfg.missingMargin?undefined:new Map(Object.entries(cfg.margin||{})),leverage=cfg.missingLeverage?undefined:new Map(Object.entries(cfg.leverage||{}));
   const receiver={store,parent:{store,marginTypePreferences:margin,leverageSettings:leverage,emitter:{emit:(name,payload)=>{assert.equal(name,'fill');effect('fill',{payload});}}},
    throttledFetchOrders:()=>effect('throttle'),isPublicDataOnly:!!cfg.public,
    log:(message,severity)=>{assert.equal(severity,types.LogSeverity.Warning);effect('warning',{message});},
    emitAndLogError:message=>effect('incomeError',{message})};
   const bindings={...types,ORDER_TYPE:spotTypes.ORDER_TYPE,v:getV,createPositionId,Date:class extends Date {static now(){return cfg.clock??1723638896789;}},
     virtualClock:{getCurrentTimeMs:()=>cfg.clock??1723638896789},ACCOUNT_LOG_MAX_WINDOW_MS:30*24*60*60*1000,ACCOUNT_LOG_PAGE_LIMIT:500,
     paginateIncomeRange(page,options){effect('paginate',{options});if(cfg.pagerSyncFail)throw original;return (async()=>{for(let i=0;i<(cfg.pageCalls??1);i++)await page({startTime:options.startTime+i,limit:17.25});if(cfg.pagerFail)throw original;return {records:cfg.records,truncated:cfg.truncated??false,exhaustedChunks:cfg.exhaustedChunks??0};})();}};
   receiver._fetchAccountLogPage=params=>{effect('page',{params});return cfg.pageFail?Promise.reject(original):Promise.resolve([]);};
   receiver.fetchIncomeHistoryRange=(start,end,type)=>{effect('range',{start,end,type});if(cfg.rangeSyncFail)throw original;return Promise.resolve().then(()=>{Object.assign(cfg.input||{},cfg.afterOptions||{});if(cfg.rangeFail)throw original;return cfg.records;});};
   if(family==='executions')receiver.parseOrder=item=>{effect('parse',{identity:contains(cfg.input,item)});if(cfg.overrideParse)return {id:'override',symbol:'Z',info:item};return method(spotFile,'KrakenSpotPrivateWebsocket','parseOrder',bindings,receiver)(item);};
   const name={fills:'handleFills',orders:'handleOpenOrders',positions:'handleOpenPositions',balances:'handleBalances',parse:'parseOrder',executions:'handleExecutions',range:'fetchIncomeHistoryRange',history:'fetchIncomeHistory'}[family];
   const spot=family==='parse'||family==='executions';
   const file=(family==='range'||family==='history')?path.join(src,'exchanges/kraken/kraken.exchange.ts'):(spot?spotFile:futuresFile);
   const fn=method(file,spot?'KrakenSpotPrivateWebsocket':family==='range'||family==='history'?'KrakenExchange':'KrakenPrivateWebsocket',name,bindings,receiver);
   let result,error,resultInfoIdentity;
   try{result=family==='range'?await fn(cfg.input.start,cfg.input.end,cfg.input.type):await fn(cfg.input);resultInfoIdentity=family==='parse'&&!!result?result.info===cfg.input:undefined;}catch(e){error={identity:e===original,name:e.name,message:e.message};}
   rows[family].push({cfg:capture(raw),result:capture(result),resultInfoIdentity,error,events:capture(events),margin:capture(margin?Object.fromEntries(margin):undefined),leverage:capture(leverage?Object.fromEntries(leverage):undefined)});
  }
 }
 process.stdout.write(JSON.stringify({header:{hashes,tz:process.env.TZ},rows}));
})().catch(e=>{console.error(e);process.exitCode=1});
