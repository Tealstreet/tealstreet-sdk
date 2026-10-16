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
const market=(symbol,active)=>({id:'id-'+symbol,symbol,base:'BASE',quote:'QUOTE',active,precision:{amount:0.125,price:0.25},limits:{amount:{min:1.5,max:17.25},leverage:{min:2.75,max:31.5}}});
const markets=[market('PF_XRPUSD',true),market('OFF',false),market('BTC/USD',true)];
const log={booking_uid:'booking-A',id:'fallback-B',contract:'pf_xrpusd',asset:'usd',date:'2026-08-14T12:34:56.789Z',info:'futures trade',execution:'execution-C',realized_pnl:'13.125',realized_funding:'-17.25',fee:'19.5',liquidation_fee:'23.75',old_balance:'29.125',new_balance:'31.75'};
async function account(raw,paramsRaw,fail=false){
 const calls=[];const original=new Error('read failure');original.code='EPRIVATE';original.response={status:429};
 let release;const pending=new Promise(resolve=>{release=resolve});
 const store={marketsBySymbol:{}};
 const fn=method(futuresFile,'KrakenExchange','_fetchAccountLogPage',{ENDPOINTS:futuresTypes.ENDPOINTS,mapKrakenAccountLog:income.mapKrakenAccountLog},{store,historyXhr:{get(endpoint,options){calls.push({endpoint,options:capture(options)});return fail?Promise.reject(original):pending;}}});
 const promise=fn(JSON.parse(paramsRaw));const prefixCalls=calls.length;
 // The resolver must see the Store at continuation time, not call time.
 store.marketsBySymbol=Object.fromEntries(markets.map(m=>[m.symbol,m]));
 release({data:value(raw)});
 let result;try{result=capture(await promise)}catch(e){result={error:e.name,message:e.message,identity:e===original};}
 return {raw,paramsRaw,fail,calls,prefixCalls,result};
}
async function token(raw,fail=false){
 const calls=[];const original=new Error('read failure');original.code='EPRIVATE';
 const fn=method(spotFile,'KrakenSpotExchange','fetchWsToken',{ENDPOINTS:spotTypes.ENDPOINTS},{xhr:{post(endpoint,body){calls.push({endpoint,body:capture(body)});return fail?Promise.reject(original):Promise.resolve({data:value(raw)});}}});
 const promise=fn();const prefixCalls=calls.length;
 let result;try{result=capture(await promise)}catch(e){result={error:e.name,message:e.message,identity:e===original};}
 return {raw,fail,calls,prefixCalls,result};
}
(async()=>{
 const accounts=[];
 for(const raw of [null,'null','false','0','"primitive"','{}','[]','[1,2]','{"10":1,"2":2,"z":3,"a":4}','{"status":"unauthorized","reason":"different reason"}','{"status":0,"reason":false}','{"reason":["bad","request"]}','{"logs":null}','{"logs":{}}','{"logs":[]}',JSON.stringify({logs:[log,{...log,contract:'MISSING'}]})])accounts.push(await account(raw,'{"startTime":1723638896789.125,"limit":17.25}'));
 for(const paramsRaw of ['{"startTime":-0,"limit":5e-324}','{"startTime":1e400,"limit":-1e400}','{"startTime":-1e-400,"limit":9007199254740993}'])accounts.push(await account('{"logs":[]}',paramsRaw));
 accounts.push(await account(null,'{"startTime":1,"limit":2}',true));
 const tokens=[];
 for(const raw of [null,'null','false','[]','{}','{"result":null}','{"result":{}}','{"result":{"token":"abc-private"}}','{"result":{"token":false}}','{"result":{"token":0}}','{"result":{"token":-0}}','{"result":{"token":""}}','{"result":{"token":1e400}}','{"result":{"token":[]}}','{"result":{"token":{"value":-0,"other":17.25}}}'])tokens.push(await token(raw));
 tokens.push(await token(null,true));
 const positions=[];
 for(const m of [[],markets,[market('',true),market('same',true),market('same',true)]]){
  const fn=method(spotFile,'KrakenSpotExchange','fetchPositions',{PositionSide:types.PositionSide,PositionMode:types.PositionMode},{store:{markets:m}});
  const promise=fn();m.push(market('AFTER-CALL',true));const result=capture(await promise);m.pop();positions.push({markets:m,result});
 }
 const deep=JSON.stringify({logs:[],ignored:Array.from({length:140}).reduce(x=>({child:x}),0)});
 const boundaries={params:await account('{"logs":[]}','{"startTime":"123","limit":"17"}'),surrogate:await token('{"result":{"token":"\\ud800"}}'),depth:await account(deep,'{"startTime":1,"limit":2}')};
 boundaries.surrogate.units=Array.from(boundaries.surrogate.result,c=>c.charCodeAt(0));delete boundaries.surrogate.result;
 process.stdout.write(JSON.stringify({header:{node:process.version,nodeMajor:Number(process.versions.node.split('.')[0]),arch:process.arch,v8:process.versions.v8,tz:process.env.TZ,hashes},markets,accounts,tokens,positions,boundaries}));
})().catch(e=>{console.error(e);process.exitCode=1});
