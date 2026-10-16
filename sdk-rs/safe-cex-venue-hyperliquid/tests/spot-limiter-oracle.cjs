'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.rateLimiter.ts'),source=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true),declaration=ast.statements.find(ts.isClassDeclaration);
assert.equal(declaration.name.text,'RateLimiter');assert.deepEqual(declaration.members.filter(ts.isPropertyDeclaration).map(n=>n.name.text),['tokens','lastRefill','capacity']);assert.equal(declaration.members.filter(ts.isConstructorDeclaration).length,1);assert.deepEqual(declaration.members.filter(ts.isMethodDeclaration).map(n=>n.name.text),['refillTokens','waitForToken']);
const code=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,useDefineForClassFields:true}}).outputText;
const inputs=JSON.parse(fs.readFileSync(path.join(__dirname,'fixtures/spot-limiter-inputs.json'),'utf8'));assert.equal(inputs.length,34,'LIMITER independent source input floor');
function value(v){switch(v){case'$undefined':return undefined;case'$nan':return NaN;case'$inf':return Infinity;case'$-inf':return-Infinity;case'$-0':return-0;default:return v}}
const P=Promise;
async function run(row){
 const trace=[],timers=[],resolves=new Set(),fixtureErrors=[],originalError={original:'limiter failure'};let clock=0,reads=0,coercions=0,receiver,raw,recursiveWeightAlias=true;
 function must(condition,message){if(!condition){const e=new assert.AssertionError({message});fixtureErrors.push(e);throw e}}
 const record=(label,...args)=>trace.push([label,...args]);
 let weight=value(row.weight);if(row.weight&&typeof row.weight==='object'){weight={valueOf(){const n=coercions++;record('coerce',n);if(row.coercionThrows)throw originalError;if(row.coercionSetsTokens&&n===0)receiver.tokens=7;return value(row.weight.values[Math.min(n,row.weight.values.length-1)])}}}
 const date={now(){must(this===date,'LIMITER source Date receiver');if(row.clockThrows){record('now-throw');throw originalError}const v=value(row.clock[Math.min(clock++,row.clock.length-1)]);record('now',v);return v}};
 const NativePromise=new Proxy(P,{construct(target,args){record('construct');const executor=args[0];must(executor.name===''&&executor.length===1,'LIMITER source executor metadata');const p=Reflect.construct(target,[(resolve,reject)=>{resolves.add(resolve);return executor(resolve,reject)}],target);Object.defineProperty(p,'then',{get(){record('get-then');if(row.thenThrows)throw originalError;return function(callback){must(callback.name===''&&callback.length===0,'LIMITER source continuation metadata');return Reflect.apply(P.prototype.then,this,[callback])}}});return p}});
 const context={exports:{}};
 for(const[name,v]of [['Date',date],['Promise',NativePromise],['setTimeout',function(callback,delay){must(this===undefined,'LIMITER source timer receiver');must(arguments.length===2,'LIMITER source timer args');must(resolves.has(callback),'LIMITER source original resolve function');record('timer',delay);if(row.timerThrows)throw originalError;timers.push([callback,delay]);return 81}]])Object.defineProperty(context,name,{get(){record('global',name);return v}});
 vm.runInNewContext(code,context);const Klass=context.exports.RateLimiter;
 let result,error,observed=[],syncError,droppedSettled=false,droppedValue;
 if(row.family==='constructor'){try{raw=new Klass();receiver=raw;result=undefined}catch(e){syncError=e}}
 else{
  raw=Object.assign(Object.create(Klass.prototype),{tokens:value(row.tokens),lastRefill:value(row.last),capacity:value(row.capacity)});
  receiver=new Proxy(raw,{get(target,key,recv){record('get',key);if(key==='refillTokens'){if(row.refillThrows)throw originalError;return function(){must(this===receiver,'LIMITER source refill receiver');record('refill');return Klass.prototype.refillTokens.call(this)}}if(key==='tokens'){const n=reads++;if(row.tokenReadChanges&&n===1)return 9}if(key==='waitForToken'&&!Object.hasOwn(target,key))return function(w){must(this===receiver,'LIMITER source recursive receiver');if(typeof weight==='object')recursiveWeightAlias&&=w===weight;record('wait');return Klass.prototype.waitForToken.call(this,w)};return Reflect.get(target,key,recv)},set(target,key,v,recv){record('set',key,v);return Reflect.set(target,key,v,recv)}});
  if(row.family==='refill'){try{Klass.prototype.refillTokens.call(receiver);result=undefined}catch(e){syncError=e}}
  else{
   const enter=()=>{const p=Klass.prototype.waitForToken.call(receiver,weight);if(row.drop){p.then(v=>{droppedSettled=true;droppedValue=v});return;}const state={status:'pending'};observed.push(state);p.then(v=>{state.status='fulfilled';state.value=v},e=>{state.status='rejected';state.error=e})};
   enter();if(row.expectedTimers.length===0&&!row.error)must(Object.is(raw.tokens,value(row.expectedTokens)),"LIMITER source eager prefix tokens");if(row.concurrent)enter();
   if(row.replace)receiver.waitForToken=function(w){must(this===receiver,'LIMITER source replacement receiver');if(typeof weight==='object')recursiveWeightAlias&&=w===weight;record('replacement');return row.recursiveReject?P.reject(originalError):P.resolve('adopted')};
   for(const index of row.fire||[]){must(index<timers.length,'LIMITER source manual timer index');record('fire',index);timers[index][0]();for(let i=0;i<32;i++)await P.resolve()}
   for(let i=0;i<32;i++)await P.resolve();
  }
 }
 if(fixtureErrors.length)throw fixtureErrors[0];
 if(row.family==='constructor')assert.deepEqual(Object.keys(raw),['tokens','lastRefill','capacity'],'LIMITER source constructor exact own field floor');
 if(row.drop){assert(droppedSettled,'LIMITER source dropped outer settled');assert.equal(droppedValue,undefined)}
 assert.deepEqual(Object.keys(raw).slice(0,3),['tokens','lastRefill','capacity'],'LIMITER source own field order');
 assert(Object.is(raw.tokens,value(row.expectedTokens)),'LIMITER source independently expected tokens '+row.name);assert(Object.is(raw.lastRefill,value(row.expectedLast)),'LIMITER source independently expected lastRefill '+row.name);assert(Object.is(raw.capacity,value(row.family==='constructor'?1200:row.capacity)),'LIMITER source native capacity');
 assert.equal(timers.length,row.expectedTimers.length,'LIMITER source independent timer floor '+row.name);row.expectedTimers.forEach((v,i)=>assert(Object.is(timers[i][1],value(v)),'LIMITER source exact native timer delay '+row.name));assert(recursiveWeightAlias,'LIMITER source original captured weight');
 if(row.family==='wait'&&!row.drop){assert.equal(observed.length,row.concurrent?2:1,'LIMITER source observer floor');for(const state of observed){assert.equal(state.status,row.pending?'pending':row.error?'rejected':'fulfilled','LIMITER source settlement '+row.name);if(row.error)assert.equal(state.error,originalError,'LIMITER source original rejection identity');else if(!row.pending)assert(Object.is(state.value,value(row.expectedResult)),'LIMITER source adopted result')}}else assert.equal(syncError,undefined);
 return {name:row.name,state:[raw.tokens,raw.lastRefill,raw.capacity],timers:timers.map(p=>p[1]),trace,observed:row.drop?['dropped']:observed.map(s=>s.status==='fulfilled'?['fulfilled',s.value]:[s.status]),recursiveWeightAlias};
}
function bits(n){if(Number.isNaN(n))return'nan';const b=new ArrayBuffer(8),d=new DataView(b);d.setFloat64(0,n,false);return d.getBigUint64(0,false).toString(16).padStart(16,'0')}
function encode(v){if(v===undefined)return['u'];if(v===null)return['null'];if(typeof v==='boolean')return['bool',v];if(typeof v==='number')return['number',bits(v)];if(typeof v==='string')return['string',Array.from({length:v.length},(_,i)=>v.charCodeAt(i))];if(typeof v==='function')return['function'];if(Array.isArray(v))return['array',v.map(encode)];return['object',Object.keys(v).map(k=>[k,encode(v[k])])]}
(async()=>{const native=[];let last=-1;for(const row of inputs){native.push(await run(row));last++}assert.equal(native.length,34,'LIMITER independently completed native floor');assert.equal(last,33,'LIMITER source final iteration');const cases=native.map(encode);assert.equal(cases.length,34,'LIMITER independent source result floor');process.stdout.write(JSON.stringify({cases,last_index:last}))})().catch(e=>{console.error(e);process.exitCode=1});
