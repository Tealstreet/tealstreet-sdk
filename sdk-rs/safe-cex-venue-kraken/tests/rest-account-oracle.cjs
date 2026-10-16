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
const configs=require('./rest-account-fixtures.cjs');
(async()=>{
 const output={};
 for(const [family,rows] of Object.entries(configs)){
  output[family]=[];
  for(const cfg of rows){
   const events=[];const original=new Error('read failure');
   const store=structuredClone(cfg.store||{});const margin=new Map(Object.entries(cfg.margin||{}));const leverage=new Map(Object.entries(cfg.leverage||{}));
   let calls=0;let release;let reject;let pending;
   const receiver={store,marginTypePreferences:margin,leverageSettings:leverage,isPublicDataOnly:!!cfg.public,isDisposed:false,
    log(message,severity){events.push({kind:'warn',message,severity});if(cfg.warnFail)throw original;},
    emitAndLogError(message){events.push({kind:'emit',message});if(cfg.emitFail)throw original;},
    async fetchBalance(){events.push({kind:'fetchBalance'});await new Promise(r=>{release=r});if(cfg.fail)throw original;return cfg.balance;}
   };
   store.update=patch=>{events.push({kind:'update',patch});if(cfg.updateFail)throw original;};
   const read=(endpoint,options)=>{
    events.push({kind:'read',endpoint,options});const n=calls++;
    if(cfg.syncFail)throw original;
    if(n===0){pending=new Promise((r,j)=>{release=r;reject=j});return pending;}
    if(cfg.failAt===n)return Promise.reject(original);
    return Promise.resolve({data:cfg.pages?cfg.pages[Math.min(n,cfg.pages.length-1)]:cfg.data});
   };
   receiver.xhr={get:read,post:read};
   let file=family.startsWith('spot')?spotFile:futuresFile;
   const name={futuresMarkets:'fetchMarkets',spotMarkets:'fetchMarkets',spotTickers:'fetchTickers',spotOrders:'fetchOrders',futuresPositions:'fetchPositions',futuresBalance:'fetchBalance',spotRefresh:'refreshBalance',futuresFills:'fetchHistoricalFills'}[family];
   const bindings={v:getV,...types,...spotTypes,...precision,mapKrakenFills:fills.mapKrakenFills,ENDPOINTS:file===spotFile?spotTypes.ENDPOINTS:futuresTypes.ENDPOINTS,virtualClock:{getCurrentTimeMs:()=>1723638896789},FILLS_MAX_PAGES:50,FILLS_PAGE_SIZE:100,Date:class extends Date{static now(){return 1723638896789;}},console:{trace(e){events.push({kind:'trace',identity:e===original});if(cfg.traceFail)throw original;}}};
   const fn=method(file,file===spotFile?'KrakenSpotExchange':'KrakenExchange',name,bindings,receiver);
   const promise=fn(cfg.opts||{});const prefix=events.length;
   for(const [k,v] of Object.entries(cfg.after||{}))store[k]=structuredClone(v);
   if(cfg.disposed)receiver.isDisposed=true;
   if(release){if(cfg.fail&&family!=='spotRefresh')reject(original);else release(family==='spotRefresh'?undefined:{data:cfg.pages?cfg.pages[0]:cfg.data});}
   let result;let error;
   try{result=await promise;}catch(e){error={identity:e===original,name:e.name};}
   const current=family==='spotRefresh'?store.balance:store[{futuresMarkets:'markets',spotMarkets:'markets',spotTickers:'tickers',spotOrders:'orders',futuresPositions:'positions',futuresBalance:'balance'}[family]];
   output[family].push({cfg:capture(cfg),events:capture(events),result:capture(result),error,fallback:family!=='spotRefresh'&&result===current,prefix,margin:capture(Object.fromEntries(margin))});
  }
 }
 process.stdout.write(JSON.stringify({header:{hashes,tz:process.env.TZ},output}));
})().catch(e=>{console.error(e);process.exitCode=1});
