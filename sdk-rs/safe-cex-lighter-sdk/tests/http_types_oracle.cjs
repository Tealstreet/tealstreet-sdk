const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function source(part){const p=path.join(root,'packages/lighter-sdk/src/client/'+part+'.ts');return ts.createSourceFile(p,fs.readFileSync(p,'utf8'),ts.ScriptTarget.Latest,true);}
const http=source('http-types'),ref=source('referral-api');
function decl(src,name){const hits=src.statements.filter(n=>n.name?.text===name);assert.equal(hits.length,1,name+' actual declaration');return hits[0];}
function shape(n){if(ts.isTypeLiteralNode(n))return n.members.map(m=>[m.name.text,!!m.questionToken,shape(m.type)]);if(ts.isArrayTypeNode(n))return{array:shape(n.elementType)};if(ts.isTypeReferenceNode(n)&&n.typeName.getText()==='Array')return{array:shape(n.typeArguments[0])};return n.getText().replace(/\s+/g,' ');}
function schema(src,name){return decl(src,name).members.map(m=>[m.name.text,!!m.questionToken,shape(m.type)]);}
function alias(name){const n=decl(http,name).type;return{params:n.parameters.map(p=>[p.name.text,shape(p.type)]),returns:shape(n.type)};}
const schemas={RequestInterceptor:alias('RequestInterceptor'),ResponseInterceptor:alias('ResponseInterceptor'),ErrorInterceptor:alias('ErrorInterceptor'),Interceptors:schema(http,'Interceptors'),ReferralPoints:schema(ref,'ReferralPoints'),ReferralPointsResponse:schema(ref,'ReferralPointsResponse')};
const errorSource=decl(http,'LighterApiError').getText(http),code=ts.transpileModule(errorSource+'\nLighterApiError',{reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}});assert(!code.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error));
function units(s){return Array.from({length:s.length},(_,i)=>s.charCodeAt(i));}
function number(n){const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'NaN':b.toString('hex');}
function observe(v){if(typeof v==='string')return{units:units(v)};if(typeof v==='number')return{number:number(v)};if(v===undefined)return{undefined:true};if(v===null)return null;if(Array.isArray(v))return v.map(observe);if(typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,observe(v)]));return v;}
const errors=[];
for(let i=0;i<4;i++){
 const context=vm.createContext({exports:{}});vm.runInContext('globalThis.traces=[]; globalThis.originalCapture=Error.captureStackTrace; Error.captureStackTrace='+(i===0?'undefined':`function(target,constructor){traces.push({name:target.name,code:target.code,message:target.message,requestId:target.requestId,statusCode:target.statusCode,constructorName:constructor.name});originalCapture(target,constructor);}`),context);
 const ErrorClass=vm.runInContext(code.outputText,context),message=['plain','\ud800 message','','astral 😀'][i],inputCode=[undefined,'','custom\udfff','LAST'][i],details=i===0?undefined:{opaque:true,live:{count:7}},requestId=i===0?undefined:['','request\ud800','trace'][i-1],status=[undefined,-0,NaN,Infinity][i];
 const e=new ErrorClass(message,inputCode,details,requestId,status);if(details)details.live.count=8;
 errors.push({input:observe({message,code:inputCode,requestId,statusCode:status,detailsPresent:details!==undefined}),output:observe({name:e.name,message:e.message,code:e.code,requestId:e.requestId,statusCode:e.statusCode,detailsSame:e.details===details,liveCount:details?.live.count}),isError:vm.runInContext('Error',context).prototype.isPrototypeOf(e),messageEnumerable:Object.getOwnPropertyDescriptor(e,'message')?.enumerable??false,ownKeys:Object.keys(e),trace:observe(context.traces)});
}
const failures=[];
for(const setting of ['function(){throw Object.assign(new Error("capture fault"),{marker:17})}','true']){const context=vm.createContext({exports:{}});vm.runInContext('Error.captureStackTrace='+setting,context);const C=vm.runInContext(code.outputText,context);try{new C('message');assert.fail('capture should fail');}catch(e){failures.push({name:e.name,message:units(e.message),marker:e.marker??null});}}
{
 const context=vm.createContext({exports:{}}),C=vm.runInContext(code.outputText,context),cause={marker:17};
 let calls=0;
 Object.defineProperty(C.prototype,'name',{set(value){assert.equal(value,'LighterApiError');calls++;throw cause;}});
 assert.throws(()=>new C('assigned name'),error=>error===cause);
 assert.equal(calls,1,'source name assignment invokes the inherited setter');
}
const points=[];
for(let i=0;i<4;i++){
 const p={accountIndex:['1','0001','\ud800-index','2'.repeat(100)][i],totalPoints:[1.25,-0,NaN,Infinity][i]};
 if(i>0)Object.assign(p,{availablePoints:[0,-0,-Infinity,12.5][i],pendingPoints:[0,NaN,Infinity,-0][i],referralCount:[0,1.5,-0,NaN][i],tier:['','\udfff','bronze',''][i],rewards:{totalUsd:i===2?undefined:'usd-'+i,byToken:i===1?[]:[{token:'token\ud800',amount:'000.12'},{token:'token\ud800',amount:'-100000000000000000000'},{token:'other',amount:'3'}]}});
 const response={referralPoints:p};points.push({point:observe(p),same:response.referralPoints===p});
}
(async()=>{const callbacks=[];for(const kind of ['request','response','error'])for(let i=0;i<4;i++){
 const v={opaque:kind+'-'+i},failure={thrown:true},fn=i===3?()=>{throw failure}:i===1&&kind!=='error'?async x=>x:i===2?x=>{x.changed=true;return x}:x=>x;
 const list=[fn,x=>({second:x})],config={[kind]:list},got=config[kind][0];let output;
 try{const result=got(v),promise=result instanceof Promise,resolved=promise?await result:result;output={same:resolved===v,promise,changed:v.changed===true};}catch(e){output={thrownSame:e===failure}}
 callbacks.push({kind,variant:i,sameCallback:got===fn,sameList:config[kind]===list,output});
 }
 const configs=[{}, {request:[]}, {response:[],error:[]}, {request:[()=>1,()=>2],response:[()=>3],error:[()=>4,()=>5,()=>6]}].map(c=>Object.fromEntries(Object.entries(c).map(([k,v])=>[k,v.length])));
 process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},schemas,errors,failures,points,callbacks,configs}));
})().catch(e=>{console.error(e);process.exitCode=1;});
