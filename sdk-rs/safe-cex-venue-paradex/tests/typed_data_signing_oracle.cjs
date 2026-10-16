const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const source=ts.createSourceFile('sdk.ts',fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/paradex/paradex.sdk.ts'),'utf8'),99,true);
assert.equal(source.parseDiagnostics.length,0);
const nodes=source.statements.filter(n=>ts.isFunctionDeclaration(n)&&n.name.text==='signParadexTypedData');assert.equal(nodes.length,1);
let body=nodes[0].getText(source);assert.equal(body.split("import('starknet')").length,2);body=body.replace("import('starknet')",'loadStarknet()');
const compiled=ts.transpileModule(body,{compilerOptions:{module:1,target:9},reportDiagnostics:true});assert.equal(compiled.diagnostics.length,0);
const profiles=['remote','remote-changing','local-object','local-array','empty','three','sparse','mixed','inherited','custom-map','bigint-object','unicode','missing','remote-null','string-replace','bigint-method'];
const failures=['none','get:options:payloadSigner#0','import#0','get:module:typedData#0','get:options:payloadSigner#1','get:options:key#0','get:typedData:getMessageHash#0','get:sdk:account#0','get:account:address#0','hash#0','remote#0','get:account:signMessage#0','sign#0','is-array#0','get:sig:map#0','map#0','string#0','string#1','get:sig:r#0','get:sig:s#0','stringify#0','import-reject#0','sign-reject#0','remote-reject#0','string-select#0','string-select#1','bigint-get#0','bigint-call#0'];
async function run(profile,failure){
 const trace=[],counts=new Map(),primary={marker:'original'},typed={marker:'typed'},returned={marker:'remote-result',headers:{extra:'retained'}};let signerReads=0,request=null,localInput=false,currentString=value=>String(value);
 const touch=(key,throws=true)=>{const n=counts.get(key)||0;counts.set(key,n+1);const label=key+'#'+n;trace.push(label);if(label===failure&&throws)throw primary;};
 const observe=(label,value)=>new Proxy(value,{get(t,k,r){if(typeof k==='string'&&k!=='then')touch('get:'+label+':'+k);return Reflect.get(t,k,r);}});
 let sig;
 if(profile==='bigint-method')sig=[19n,23n];else if(profile==='string-replace'){currentString=value=>'old:'+String(value);sig={get r(){currentString=value=>'new:'+String(value);return 1;},s:2};}else if(profile==='empty')sig=[];else if(profile==='three')sig=[1n,2n,3n];else if(profile==='sparse')sig=[,undefined,1n,NaN];else if(profile==='mixed')sig=[null,true,undefined,Infinity,-0,'quote"\n'];else if(profile==='unicode')sig=['lone-\ud800','snow-雪'];else if(profile==='local-array')sig=[11n,'0x22'];else if(profile==='custom-map'){sig=[11n,22n];sig.map=function(cb){touch('map');assert.equal(this,sig);return ['custom',cb(9n)];};}else if(profile==='inherited')sig=Object.create({r:31n,s:'0x42'});else if(profile==='bigint-object')sig={r:51n,s:62n};else if(profile==='missing')sig={};else sig={r:'0x11',s:'0x22'};
 if(Array.isArray(sig)&&profile!=='custom-map')sig.map=function(cb){touch('map');assert.equal(this,sig);return Array.prototype.map.call(this,cb);};
 sig=observe('sig',sig);const account=observe('account',{address:'0x1234',signMessage(input){touch('sign');assert.equal(this,account);localInput=input===typed;if(failure==='sign-reject#0'){touch('sign-reject',false);return Promise.reject(primary);}return Promise.resolve(sig);}});
 const sdk=observe('sdk',{account});let options;
 const remote=function(req){touch('remote');assert.equal(this,options);request=req;if(failure==='remote-reject#0'){touch('remote-reject',false);return Promise.reject(primary);}return Promise.resolve(profile==='remote-null'?null:returned);};
 options=observe('options',{get payloadSigner(){signerReads++;return profile.startsWith('remote')?(profile==='remote-changing'&&signerReads===1?{}:remote):undefined;},key:'key-route'});
 const typedData=observe('typedData',{getMessageHash(input,address){touch('hash');assert.equal(this,typedData);assert.equal(input,typed);assert.equal(address,'0x1234');return '0xabcd';}}),moduleValue=observe('module',{typedData});
 const module={exports:{}};const context=vm.createContext({module,exports:module.exports,bigintTouch:touch,loadStarknet(){touch('import');if(failure==='import-reject#0'){touch('import-reject',false);return Promise.reject(primary);}return Promise.resolve(moduleValue);},Array:{isArray(v){touch('is-array');return Array.isArray(v);}},get String(){touch('string-select');const selected=currentString;return function(v){touch('string');return selected(v);};},JSON:{stringify(v){touch('stringify');return JSON.stringify(v);}}});
 if(profile==='bigint-method')vm.runInContext(`const originalBigIntToString=BigInt.prototype.toString;Object.defineProperty(BigInt.prototype,'toString',{configurable:true,get(){bigintTouch('bigint-get');return function(){'use strict';bigintTouch('bigint-call');if(this!==19n&&this!==23n)throw Error('bigint receiver');if(arguments.length!==0)throw Error('bigint arguments');return originalBigIntToString.call(this);};}});`,context);
 vm.runInContext(compiled.outputText,context);
 let outcome;try{const value=await module.exports.signParadexTypedData(sdk,typed,options);outcome={ok:true,remote:value===returned,null:value===null,...(!profile.startsWith('remote')?{signature:Array.from({length:value.signature.length},(_,i)=>value.signature.charCodeAt(i))}: {})};}catch(e){assert.equal(e,primary);outcome={ok:false,original:true};}
 return {profile,failure,trace,request,localInput,outcome};
}
(async()=>{
 const cases=[];for(const profile of profiles)for(const failure of failures)cases.push(await run(profile,failure));assert.equal(cases.length,448);
 const starknet=require(require.resolve('starknet',{paths:[root]}));
 const payloads=[];
 const signerNode=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='ParadexRequestSigner');
 const jwtNode=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='ParadexJWTManager');
 const builders=ts.transpileModule(signerNode.getText(source)+'\n'+jwtNode.getText(source),{compilerOptions:{module:1,target:9},reportDiagnostics:true});assert.equal(builders.diagnostics.length,0);
 const builderModule={exports:{}};
 vm.runInNewContext(builders.outputText,{module:builderModule,exports:builderModule.exports,BigNumber:require('bignumber.js'),Date:{now:()=>1700000123456},BASE_URL:{livenet:'http://unused.invalid',testnet:'http://unused.invalid'},console:{error(...args){assert.ok(args.length);}},signParadexTypedData:async(_account,data)=>{payloads.push(data);return{signature:'captured'};},fetch:async()=>({ok:true,json:async()=>({jwt_token:'captured'})})});
 const sdkAccount={account:{address:'0x1234'},config:{paradexChainId:'0x534e5f5345504f4c4941'}};
 const builder=new builderModule.exports.ParadexRequestSigner(sdkAccount,{});
 const parameters={id:'123456',market:'BTC-USD-PERP',side:'BUY',type:'LIMIT',size:'0.01234567',price:'61234.56789'};
 await builder.signOrder(parameters);await builder.signModifyOrder(parameters);
 const jwt=new builderModule.exports.ParadexJWTManager(sdkAccount,false,'0x123',{});await jwt.generateNewToken();
 assert.deepEqual(payloads.map(p=>p.primaryType),['Order','ModifyOrder','Request']);
 const crypto=[];
 for(const address of ['0x1234','0x5678'])for(const key of ['0x123','0x456'])for(const data of payloads){
  const account=new starknet.Account({nodeUrl:'http://unused.invalid'},address,key);const module={exports:{}};
  vm.runInNewContext(compiled.outputText,{module,exports:module.exports,loadStarknet:async()=>starknet});
  const local=await module.exports.signParadexTypedData({account},data,{});let request;
  const remoteResult={signature:'opaque',headers:{extra:'kept'}};const remote=await module.exports.signParadexTypedData({account},data,{key,payloadSigner(req){request=req;return remoteResult;}});assert.equal(remote,remoteResult);
  crypto.push({address,key,raw:JSON.stringify(data),hash:starknet.typedData.getMessageHash(data,address),signature:local.signature,request});
 }
 assert.equal(crypto.length,12);
 process.stdout.write(JSON.stringify({profiles,failures,cases,crypto}));
})().catch(e=>{console.error(e);process.exitCode=1;});
