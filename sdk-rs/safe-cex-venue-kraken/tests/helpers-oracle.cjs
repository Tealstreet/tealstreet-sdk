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

const crypto = require('node:crypto');
const {pathToFileURL} = require('node:url');
function declarations(file,names,bindings) {
  const source=fs.readFileSync(file,'utf8');
  const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
  const selected=names.map(name=>{
    const matches=ast.statements.filter(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>ts.isIdentifier(d.name)&&d.name.text===name));
    assert.equal(matches.length,1,`actual TS declaration ${name}`);
    assert.equal(matches[0].declarationList.declarations.length,1,`single declaration ${name}`);
    return matches[0].getText(ast);
  }).join('\n');
  const compiled=ts.transpileModule(selected,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
  assert.equal((compiled.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0,'actual declarations compile');
  const js=compiled.outputText;
  return {functions:vm.runInNewContext(js+`;({${names.join(',')}})`,bindings,{filename:file}),sha256:crypto.createHash('sha256').update(source).digest('hex')};
}
function capture(value) {
 if(value===undefined)return {undefined:true};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {number:Number.isNaN(value)?'7ff8000000000000':b.toString('hex')};}
 if(Array.isArray(value))return value.map(capture);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,capture(v)]));
 return value;
}
function attempt(fn){try{return capture(fn())}catch(e){return {error:e.name,message:e.message}}}
(async()=>{
 const types=load(path.join(src,'types.ts'));
 const futures=declarations(path.join(src,'exchanges/kraken/kraken.exchange.ts'),['parseTriggerSignal'],{OrderStopTriggerType:types.OrderStopTriggerType});
 const api=declarations(path.join(src,'exchanges/kraken/kraken.api.ts'),['sort','createKrakenApiError'],{});
 const nanosource=require.resolve('nanoid',{paths:[path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH)]});
 const nanoPackage=JSON.parse(fs.readFileSync(path.join(path.dirname(nanosource),'package.json'),'utf8'));
 assert.equal(nanoPackage.version,'6.0.1','actual nanoid version');
 assert.equal(crypto.createHash('sha256').update(fs.readFileSync(nanosource)).digest('hex'),'4fde1e51160adc20b8a631138a2b021f5a88f488cff57a0e12f87017a557f64e','actual nanoid source checksum');
 assert.equal(new Intl.Collator().resolvedOptions().locale,'en-US','root-comparison host locale');
 const nanoid=await import(pathToFileURL(nanosource));
 let state=0x13579bdf; const draws=[];let fail=false;
 Object.defineProperty(globalThis,'crypto',{configurable:true,value:{getRandomValues(buffer){
   draws.push(buffer.length);if(fail){fail=false;throw new Error('entropy failure')}
   for(let i=0;i<buffer.length;i++){state^=state<<13;state^=state>>>17;state^=state<<5;buffer[i]=state&255;}return buffer;
 }}});
 const spot=declarations(path.join(src,'exchanges/kraken/krakenspot.exchange.ts'),['KRAKEN_SPOT_CL_ORD_ID_LENGTH','createKrakenSpotClientOrderId'],{customAlphabet:nanoid.customAlphabet});
 const ids=[];
 for(const failAt of [-1,17]) {
  state=0x13579bdf;fail=false;draws.length=0;
  const fresh=declarations(path.join(src,'exchanges/kraken/krakenspot.exchange.ts'),['KRAKEN_SPOT_CL_ORD_ID_LENGTH','createKrakenSpotClientOrderId'],{customAlphabet:nanoid.customAlphabet}).functions.createKrakenSpotClientOrderId;
  const cases=[];
  for(let i=0;i<1320;i++){if(i===failAt)fail=true;const before=draws.length;cases.push({result:attempt(()=>fresh()),draws:draws.slice(before)});}
  ids.push({failAt,cases});
 }
 const triggerInputs=[null,'null','false','0','-0','true','1','""','"MARK"','"Spot"','"last"','"index"','" mark"','"mark "','"marK"','[]','["spot"]','["last","mark"]','{}','{"toString":null}'];
 const triggers=triggerInputs.map(raw=>({raw,result:attempt(()=>futures.functions.parseTriggerSignal(raw===null?undefined:JSON.parse(raw)))}));
 const keys=['','a','A','z','Z','_','-','2','10','é','e\u0301','E','ä','ø','東','阿','𠀀','𠮷','💰','中','国','國','a-b','a_b','a b','a0','a10','a2','ß','ss'];
 const sorts=[];for(const left of keys)for(const right of keys)sorts.push({left,right,result:api.functions.sort(left,right)});
 const errors=[];
 for(const message of ['failure','', 'Error: already-prefixed','💰'])for(const responseRaw of [null,'null','false','{"status":429,"data":{"error":"rate limit"}}'])for(const causeRaw of [null,'null','0','{"message":"original","code":"EOTHER"}']){
  const response=responseRaw===null?undefined:JSON.parse(responseRaw);const cause=causeRaw===null?undefined:JSON.parse(causeRaw);
  const error=api.functions.createKrakenApiError(message,response,cause);
  errors.push({message,responseRaw,causeRaw,result:{name:error.name,message:error.message,string:String(error),response:capture(error.response),cause:capture(error.cause),hasResponse:Object.hasOwn(error,'response'),hasCause:Object.hasOwn(error,'cause'),responseIdentity:response===error.response,causeIdentity:cause===error.cause}});
 }
 const response={status:403};const cause={code:'before'};const alias=api.functions.createKrakenApiError('alias',response,cause);response.status=429;cause.code='after';
 const surrogate=JSON.parse('"\\ud800"');
 const boundaries={trigger:attempt(()=>futures.functions.parseTriggerSignal(surrogate)),sort:api.functions.sort(surrogate,surrogate),errorMessageUnits:Array.from(api.functions.createKrakenApiError(surrogate).message,c=>c.charCodeAt(0)),alias:{response:alias.response,cause:alias.cause},sourceOverrideSize:spot.functions.createKrakenSpotClientOrderId(1).length};
 process.stdout.write(JSON.stringify({header:{node:process.version,arch:process.arch,v8:process.versions.v8,icu:process.versions.icu,locale:new Intl.Collator().resolvedOptions().locale,nanoid:nanoPackage.version,nanoidSha256:crypto.createHash('sha256').update(fs.readFileSync(nanosource)).digest('hex'),futuresSha256:futures.sha256,apiSha256:api.sha256,spotSha256:spot.sha256},triggers,sorts,errors,ids,boundaries}));
})().catch(e=>{console.error(e);process.exitCode=1;});
