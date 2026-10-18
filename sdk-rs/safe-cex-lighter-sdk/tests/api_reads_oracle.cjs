const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function source(name){return ts.createSourceFile(name,fs.readFileSync(path.join(root,'packages/lighter-sdk/src/client',name+'.ts'),'utf8'),ts.ScriptTarget.Latest,true);}
const api=source('api-client'),order=source('order-api'),ref=source('referral-api'),http=source('http-types');
function classDecl(s,n){return s.statements.find(x=>x.name?.text===n);}
function compile(text){const out=ts.transpileModule(text,{reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}});assert(!out.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error));return out.outputText;}
const context=vm.createContext({exports:{},Date:{now:()=>123456},btoa:undefined,Buffer:undefined});
vm.runInContext(compile(classDecl(http,'LighterApiError').getText(http)),context);
const printer=ts.createPrinter();
function method(src,cls,name){const m=classDecl(src,cls).members.find(m=>m.name?.text===name);assert(m,name);const f=ts.factory.createFunctionDeclaration([ts.factory.createModifier(ts.SyntaxKind.AsyncKeyword)],undefined,name,undefined,m.parameters,m.type,m.body);return vm.runInContext(compile(printer.printNode(ts.EmitHint.Unspecified,f,src))+'\n'+name,context);}
const fns={referral:method(ref,'ReferralApi','getReferralPoints'),nonce:method(api,'LighterApiClient','getNextNonce'),keys:method(api,'LighterApiClient','getApiKeys'),fee:method(api,'LighterApiClient','getTransferFeeInfo'),orders:method(order,'OrderApi','getOrderBookOrders')};
function encode(v){if(v===undefined)return{$undefined:true};if(typeof v==='bigint')return{$bigint:String(v)};if(typeof v==='string')return{$units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};if(typeof v==='number'){let b=Buffer.alloc(8);b.writeDoubleBE(v);return{$number:Number.isNaN(v)?'NaN':b.toString('hex')};}if(Array.isArray(v))return v.map(encode);if(v!==null&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)]));return v;}
const cases=[];
function add(kind,args,payload){cases.push({kind,args,payload});}
for(const auth of [undefined,'','token\ud800']){add('referral',[123456789012345678901234567890n,auth],{success:true,data:{referralPoints:{totalPoints:-0}}});}
for(const payload of [{success:false},{success:true,data:null,error:{message:'bad\udfff',code:''}},{success:true,data:{opaque:'retained'}}])add('referral',[-2n,''],payload);
for(const code of [200,'200',0,'0',false])for(const nonce of ['900719925474099312345',0,false])add('nonce',[12n,-0],{code,nonce,success:true,data:{nextNonce:'44'}});
for(const payload of [{code:200,nonce:'bad'},{success:false,error:{code:'CUSTOM',message:'custom'}},{code:0,message:'',error:{code:'',message:''}},null,{success:true,data:{nextNonce:'-100'}}])add('nonce',[-1234567890123456789n,NaN],payload);
for(const kind of ['keys','fee'])for(const payload of [{success:false},{success:true,data:null,error:{message:'bad\ud800',code:'ERR'}},{success:true,data:kind==='keys'?{apiKeys:[{index:0},{index:0}]}:{fee:'99999999999999999999999',minAmount:'-1',maxAmount:'0x10',feePercentage:-0}},{success:true,data:kind==='keys'?{}:{fee:'1',minAmount:'bad',maxAmount:'3'}}])add(kind,kind==='keys'?[42n,NaN]:[42n,-7n,'auth\udfff'],payload);
for(const limit of [undefined,0,-0,NaN,Infinity,-2])add('orders',[-0,limit,{headers:{custom:'header'},params:{limit:'override',market_id:'bad',extra:'x'}}],{code:200,bids:[],asks:[{price:'1'}],timestamp:0});
for(const payload of [{success:true,data:{retained:true}},{code:'200',bids:[]},{code:200,asks:[],bids:'truthy',timestamp:1},{code:200,bids:[],asks:null,timestamp:-0},null,{error:{message:'nested'},message:'outer'},{message:'outer'},{code:200,bids:null,asks:null}])add('orders',[1e21,2,undefined],payload);
(async()=>{
 const output=[];
 for(const c of cases){const calls=[];const payload=c.payload;const client={get:(url,cfg)=>{calls.push([url,cfg]);return Promise.resolve(payload);}};const axios={get:(url,cfg)=>{calls.push([url,cfg]);return Promise.resolve({data:payload});}};
  let result;try{const value=await fns[c.kind].apply({client,axiosInstance:axios},c.args);result={ok:encode(value),same:value===payload?.data||value===payload?.data?.apiKeys};}catch(e){result={error:{name:e.name,message:encode(e.message),code:encode(e.code)}};}
  output.push({kind:c.kind,args:encode(c.args),payload:encode(payload),calls:encode(calls),result});
 }
 const helper=api.statements.find(n=>n.name?.text==='toBase64FromHex');const hexFn=vm.runInContext(compile(helper.getText(api))+'\ntoBase64FromHex',context),wire=[];
 const inputs=['','0x','00ff','0Xff','zz','fZ','-f','+f',' f','\u00a0f','\ud800a','ab'.repeat(300),'f','0xf'];
 for(const mode of ['btoa','buffer','none','throw'])for(const input of inputs){const calls=[];context.btoa=mode==='btoa'?s=>{calls.push(['btoa',Array.from(s,x=>x.charCodeAt(0))]);return Buffer.from(s,'latin1').toString('base64')}:mode==='throw'?()=>{throw new Error('encoder fault')}:undefined;context.Buffer=mode==='buffer'?{from:b=>{calls.push(['buffer',Array.from(b)]);return{toString:encoding=>{assert.equal(encoding,'base64');return Buffer.from(b).toString('base64');}}}}:undefined;
 let result;try{result={ok:encode(hexFn(input))};}catch(e){result={error:e.message};}wire.push({mode,input:encode(input),calls,result});}
 const reflective=vm.runInContext(compile(classDecl(ref,'ReferralApi').getText(ref))+'\nReferralApi',context),instance=new reflective({get(){}});
 const reflection={keys:Object.keys(instance),clientEnumerable:Object.getOwnPropertyDescriptor(instance,'client').enumerable,methodEnumerable:Object.getOwnPropertyDescriptor(reflective.prototype,'getReferralPoints').enumerable,instanceof:instance instanceof reflective,clientDeletable:delete instance.client};
 const constructorCalls=[];
 const makeClient=label=>({get(url,config){constructorCalls.push({label,url,index:config.params.accountIndex});return Promise.resolve({success:true,data:{label}});}});
 const original=makeClient('first'),replacement=makeClient('second'),constructed=new reflective(original);
 const descriptor=Object.getOwnPropertyDescriptor(constructed,'client');
 assert.equal(constructed.client,original);assert.equal(constructorCalls.length,0);
 const firstResult=await constructed.getReferralPoints(1n);
 assert.equal(Reflect.set(constructed,'client',replacement),true);
 assert.equal(constructed.client,replacement);
 const secondResult=await constructed.getReferralPoints(2n);
 assert.equal(Reflect.deleteProperty(constructed,'client'),true);
 await assert.rejects(constructed.getReferralPoints(3n),error=>error.name==='TypeError');
 const constructorOwnership={sameClient:descriptor.value===original,enumerable:descriptor.enumerable,writable:descriptor.writable,configurable:descriptor.configurable,results:[firstResult.label,secondResult.label],calls:constructorCalls,deleted:!Object.hasOwn(constructed,'client')};
 assert.deepEqual(constructorCalls,[{label:'first',url:'/api/v1/referral/points',index:'1'},{label:'second',url:'/api/v1/referral/points',index:'2'}]);
 vm.runInContext("BigInt.prototype.toString=function(){return 'patched-bigint';};Number.prototype.toString=function(){return 'patched-number';}",context);
 const overrides=[];
 for(const kind of Object.keys(fns)){const c=cases.find(c=>c.kind===kind),calls=[];const client={get:(url,cfg)=>{calls.push([url,cfg]);return Promise.resolve(c.payload);}},axios={get:(url,cfg)=>{calls.push([url,cfg]);return Promise.resolve({data:c.payload});}};let outcome;try{await fns[kind].apply({client,axiosInstance:axios},c.args);outcome={ok:true};}catch(e){outcome={error:e.message};}overrides.push({kind,calls:encode(calls),outcome});}
 context.btoa=s=>Buffer.from(s,'latin1').toString('base64');context.parseInt=()=>17;const wireOverride=encode(hexFn('00ff'));
 process.stdout.write(JSON.stringify({cases:output,wire,wireOverride,reflection,constructorOwnership,overrides,referralClass:classDecl(ref,'ReferralApi').members.map(m=>m.name?.text??'constructor')}));
})().catch(e=>{console.error(e);process.exitCode=1;});
