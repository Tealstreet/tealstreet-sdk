'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||require.resolve('typescript',{paths:[root]}));
const base='packages/safe-cex/src/exchanges/hyperliquid/';
const read=file=>fs.readFileSync(path.join(root,base+file),'utf8');
const transpile=code=>ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
function init(name){const sf=ts.createSourceFile('api.ts',read('hyperliquid.api.ts'),99,true);const d=sf.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).find(d=>d.name.getText(sf)===name);assert(d?.initializer,'actual source initializer '+name);return d.initializer.getText(sf)}
function load(file,bindings={}){const exports={};vm.runInNewContext(transpile(read(file)),{exports,require(name){assert.equal(name,'./hyperliquid.types');return load('hyperliquid.types.ts')},...bindings});return exports}
let now=0,timers=[];
const rate=load('hyperliquid.rateLimiter.ts',{Date:{now:()=>now},setTimeout(resolve,ms){timers.push(ms);now+=ms;resolve();return 1}});
const order=load('hyperliquid.order-response.ts');
const out={};let interceptor;const waits=[];
const context={exports:out,ENDPOINTS:{EXCHANGE:'/exchange'},...order,isSigningCaptureRequest:c=>c.capture,hyperliquidRestRateLimiter:{async waitForToken(weight){waits.push(weight)}},getHyperliquidRestRequestWeight:rate.getHyperliquidRestRequestWeight};
vm.runInNewContext(transpile(['getRequestActionType','getOrderErrorTelemetry','addResponseInterceptor','addRateLimitInterceptor'].map(name=>'const '+name+'=('+init(name)+');').join('\n')+'\nexports.response=addResponseInterceptor;exports.pacing=addRateLimitInterceptor;'),context);
if(process.argv[2]==='--cloid-weights'){
 const rows=[];
 for(const mode of ['raw','value'])for(const count of [39,40,80]){
  const body={action:{type:'cancelByCloid',cancels:Array(count).fill(null)}};
  const weight=rate.getHyperliquidRestRequestWeight({url:'/exchange',data:mode==='raw'?JSON.stringify(body):body});
  const bytes=Buffer.alloc(8);bytes.writeDoubleBE(weight);
  rows.push({mode,count,weight,weightBits:bytes.toString('hex')});
 }
 process.stdout.write(JSON.stringify(rows));
}else (async()=>{
 out.pacing({interceptors:{request:{use(f){interceptor=f}}}});
 const role='{"type":"userRole"}',batch=JSON.stringify({action:{type:'order',orders:Array(40).fill(null)}});
 const raw=[role,JSON.stringify(role),JSON.stringify(batch),'{"type":"userRole","unused":"\\ud800"}','{"type":"userRole","unused":'+'['.repeat(140)+'0'+']'.repeat(140)+'}', '{"type":"userRole","unused":"\\ud83d\\ude00"}','null','not json',role];
 const expected=[60,20,1,60,60,60,20,20,null];assert.equal(raw.length,9,'ENCODED independent source input floor');
 let completed=0;
 for(let i=0;i<raw.length;i++){const config={url:i===2?'/exchange':'/info',data:raw[i],capture:i===8};const before=waits.length;assert.equal(await interceptor(config),config,'ENCODED source original config identity');assert.equal(waits.length,before+(i===8?0:1),'ENCODED source capture/one wait');if(i!==8)assert.equal(waits.at(-1),expected[i],'ENCODED source exact weight '+i);completed++}
 assert.equal(completed,9,'ENCODED completed source floor');assert.equal(waits.length,8,'ENCODED independent source limiter call count');
 const telemetryInputs=[{action:{type:'order'},nonce:1},role,JSON.stringify(role),'{"action":{"type":"order"},"unused":"\\ud800"}','not json'];
 assert.equal(telemetryInputs.length,5,'TELEMETRY independent source input floor');let observations=0;
 for(const data of telemetryInputs){const marker={native:17};let provider=0,emissions=0;const exchange={getOrderErrorTelemetry(actual,message){provider++;assert.equal(actual,data,'TELEMETRY source original data identity');assert.equal(message,'venue rejected','TELEMETRY source message');return marker},emitAndLogError(message,unused,context){emissions++;assert.equal(message,'venue rejected');assert.equal(unused,undefined);assert.equal(context,marker,'TELEMETRY source returned context identity')}};let response;out.response({interceptors:{response:{use(f){response=f}}}},exchange);await assert.rejects(response({config:{url:'/exchange',data},status:200,data:{status:'err',response:'venue rejected'}}),e=>e.message==='venue rejected');assert.equal(provider,1,'TELEMETRY source one provider');assert.equal(emissions,1,'TELEMETRY source one emission');observations++}
 assert.equal(observations,5,'TELEMETRY completed source floor');
 const args=[[],[undefined],[0],[-0],[NaN],[-2],[Infinity],[2],[1.5]],delays=[50,50,50,50,50,50,50,100,75];assert.equal(args.length,9,'DEFAULT independent source input floor');let defaults=0;
 for(let i=0;i<args.length;i++){now=0;timers=[];const limiter=new rate.RateLimiter();await limiter.waitForToken(...args[i]);await limiter.waitForToken(1);assert.deepEqual(timers,[delays[i]],'DEFAULT source reservation '+i);defaults++}
 assert.equal(defaults,9,'DEFAULT completed source floor');
 console.log(JSON.stringify({encoded:completed,encoded_stop:'capture',telemetry:observations,telemetry_stop:'malformed raw text',defaults,defaults_stop:'fractional weight1.5'}));
})().catch(error=>{console.error(error);process.exitCode=1});
