// Actual function bodies; instrument host primitives and opaque property effects.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),vm=require('node:vm'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const directory=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/hyperliquid');
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i)),bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(Number.isNaN(n)?NaN:n);return b.toString('hex');};
const revoked=new WeakSet(),unproxy=new WeakMap();
function makeRevoked(){const {proxy,revoke}=Proxy.revocable({},{});revoke();revoked.add(proxy);return proxy;}
function native(v){if(v&&typeof v==='object'&&revoked.has(v))return ['opaque','revoked'];if(v===undefined)return ['undefined'];if(v===null)return ['null'];if(typeof v==='boolean')return ['bool',v];if(typeof v==='number')return ['number',bits(v)];if(typeof v==='string')return ['string',units(v)];if(typeof v==='symbol')return ['opaque','symbol'];if(typeof v==='bigint')return ['opaque','bigint'];if(typeof v==='function')return ['opaque','function'];v=unproxy.get(v)||v;if(Array.isArray(v))return ['array',v.map(native)];return ['object',Object.keys(v).map(k=>{const d=Object.getOwnPropertyDescriptor(v,k);return [units(k),Object.hasOwn(d,'value')?native(d.value):['opaque','accessor']];})];}
let calls=[],depth=0,record=false,special=null,stringIndex=0,errorIds=new WeakMap(),nextErrorId=3;
const sentinels=[null,{id:1},{id:2}];
function errorId(error){const sentinel=sentinels.indexOf(error);if(sentinel>0)return sentinel;if(!errorIds.has(error))errorIds.set(error,nextErrorId++);return errorIds.get(error);}
function invoke(name,arg,operation){const before=depth;depth++;try{const value=operation();if(record&&before===0)calls.push({name,arg,result:native(value)});return value;}catch(error){if(record&&before===0)calls.push({name,arg,error:errorId(error)});throw error;}finally{depth--;}}
const wrapped=new WeakMap();
function proxy(value){if(!value||typeof value!=='object')return value;if(wrapped.has(value))return wrapped.get(value);if(revoked.has(value))return value;const p=new Proxy(value,{get(target,key){const operation=()=>proxy(Reflect.get(target,key));return record&&depth===0&&typeof key==='string'?invoke('get:'+key,native(target),operation):operation();}});wrapped.set(value,p);unproxy.set(p,value);return p;}
const context={exports:{},Set,Array:{isArray:value=>invoke('isArray',native(value),()=>Array.isArray(value))},JSON:{parse:text=>invoke('parse',units(text),()=>{const parsed=JSON.parse(text);return special?.parseRevoked?makeRevoked():proxy(parsed);}),stringify:value=>invoke('stringify',native(value),()=>{if(special?.jsonError)throw sentinels[1];if(special?.jsonUndefined)return undefined;if(special?.jsonEmpty)return '';return JSON.stringify(value);})},String:value=>invoke('string',native(value),()=>{const action=special?.strings?.[stringIndex++];if(typeof action==='number')throw sentinels[action];if(typeof action==='string')return action;return String(value);})};
// Make the primitive subscription.type read observable without changing its
// value: this getter returns undefined, as an absent Number.prototype.type does.
context.__primitiveType=value=>invoke('get:type',native(value),()=>undefined);
vm.runInNewContext("Object.defineProperty(Number.prototype,'type',{configurable:true,get(){return __primitiveType(Number.prototype.valueOf.call(this));}});",context);
const diagnosticPath=path.join(directory,'hyperliquid.ws-diagnostics.ts'),diagnostic=fs.readFileSync(diagnosticPath,'utf8')+'\nexports.parseJsonObject=parseJsonObject;exports.stringifyDiagnosticPayload=stringifyDiagnosticPayload;';
vm.runInNewContext(ts.transpileModule(diagnostic,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context);
const exchangePath=path.join(directory,'hyperliquid.exchange.ts'),exchange=ts.createSourceFile(exchangePath,fs.readFileSync(exchangePath,'utf8'),ts.ScriptTarget.Latest,true);
let leaves='';for(const name of ['maskHyperliquidAddress','parseHyperliquidExchangePayload']){const statement=exchange.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.text===name));assert(statement,'actual exchange leaf '+name);leaves+=statement.getText(exchange)+`\nexports.${name}=${name};\n`;}
vm.runInNewContext(ts.transpileModule(leaves,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context);
const functions=['parseJsonObject','stringifyDiagnosticPayload','formatHyperliquidWsDiagnostic','shouldEmitHyperliquidWsSendDiagnostic','shouldEmitHyperliquidWsReceiveDiagnostic','maskHyperliquidAddress','parseHyperliquidExchangePayload'];
const cases=[];
function add(name,input,scope='public',config=null,label='base'){
 special=config;stringIndex=0;calls=[];errorIds=new WeakMap();nextErrorId=3;record=false;const data=proxy(input),inputNative=native(input);record=true;let value,error;
 try{value=context.exports[name](...['formatHyperliquidWsDiagnostic','shouldEmitHyperliquidWsReceiveDiagnostic'].includes(name)?[scope,data]:[data]);}catch(e){error=errorId(e);}
 record=false;const events=calls;cases.push({name,label,scope:units(scope),input:inputNative,calls:events,...error?{error}:{native:native(value),sameInput:value===data}});
}
const base=[undefined,null,false,0,-0,NaN,Infinity,7n,Symbol('x'),function sample(){},[],{},'','bad','null','[]','{"method":"ping","channel":"error"}','\ud800😀\udfff'];
assert.equal(base.length,18,'independent base input floor');
for(const name of functions)for(const input of base){add(name,input);if(name==='shouldEmitHyperliquidWsReceiveDiagnostic')add(name,input,'private');}
assert.equal(cases.length,144,'independent seven functions/eight scope variants times18');
for(const input of [{method:'ping'},{subscription:{type:'activeAssetData'}},{subscription:{type:'other'}},{subscription:null},{subscription:7},{method:'PING'},Object.create({method:'ping'}),{get method(){throw sentinels[1];}},{get subscription(){throw sentinels[1];}},{subscription:{get type(){throw sentinels[1];}}}])add('shouldEmitHyperliquidWsSendDiagnostic',input,'public',null,'property/read effects');
for(const channel of ['error','notification','orderUpdates','subscribed','subscriptionResponse','SubscriptionResponse','userEvents','userFills','userFundings','userNonFundingLedgerUpdates','ERROR',false,null])for(const scope of ['public','private'])add('shouldEmitHyperliquidWsReceiveDiagnostic',{channel},scope,null,'channel membership');
for(const scope of ['public','private'])add('shouldEmitHyperliquidWsReceiveDiagnostic',{get channel(){throw sentinels[1];}},scope,null,'channel getter error');
const address='0x'+'aB12'.repeat(10);
for(const input of [address,'('+address+') '+address,'_'+address,address+'_','a'+address,address+'0','é'+address+'雪','\ud800'+address+'\udfff','0X'+address.slice(2),address.slice(0,-1),'x '+address+' y'])add('formatHyperliquidWsDiagnostic',input,'public',null,'UTF16 regex boundary');
for(const length of [1,8,14,15,16])add('maskHyperliquidAddress','\ud800'+('A'.repeat(length-1)),'public',null,'UTF16 slice length');
for(const config of [{jsonUndefined:true,strings:[1,'second']},{jsonUndefined:true,strings:[1,2]},{jsonError:true,strings:['fallback']},{jsonEmpty:true}])add('stringifyDiagnosticPayload',{},'public',config,'fresh String fallback');
assert.equal(cases.length,202,'independent144+10+26+2+11+5+4case floor');
for(const name of functions){add(name,makeRevoked(),'public',null,'revoked object classifier');if(name==='shouldEmitHyperliquidWsReceiveDiagnostic')add(name,makeRevoked(),'private',null,'revoked object classifier');}
for(const name of ['parseJsonObject','shouldEmitHyperliquidWsSendDiagnostic','shouldEmitHyperliquidWsReceiveDiagnostic','parseHyperliquidExchangePayload']){add(name,'{}','public',{parseRevoked:true},'parse try revoked return');if(name==='shouldEmitHyperliquidWsReceiveDiagnostic')add(name,'{}','private',{parseRevoked:true},'parse try revoked return');}
assert.equal(cases.length,215,'independent202+8revoked+5parse-try witnesses');
console.log(JSON.stringify({functions,cases,iterationStop:214,scope:'actual source leaves with host primitive/property/error effects; native values first; intrinsic implementations delegated to host'}));
