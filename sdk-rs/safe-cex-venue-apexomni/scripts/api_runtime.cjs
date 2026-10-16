const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),{createRequire}=require('node:module');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),dependency=createRequire(path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH,'package.json'));
const root=path.resolve(__dirname,'../../../packages/safe-cex/src'),file=path.join(root,'exchanges/apexomni/apexomni.api.ts');
function declarations(file){const text=fs.readFileSync(file,'utf8'),tree=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);return tree.statements.filter(n=>ts.isFunctionDeclaration(n)||ts.isVariableStatement(n)).map(n=>n.getText(tree)).join('\n');}
const helperFile=path.join(root,'exchanges/apexomni/apexomni.utils.ts');const helperText=fs.readFileSync(helperFile,'utf8'),helperTree=ts.createSourceFile(helperFile,helperText,ts.ScriptTarget.Latest,true);const helper=helperTree.statements.find(n=>n.name?.text==='hmacSha256Base64').getText(helperTree);
const {AxiosHeaders}=dependency('axios');let ops=[],requestHandler,responseHandler;
const record=(name,...args)=>ops.push([name,...args]);
const xhr={interceptors:{request:{use:callback=>{record('register:request');requestHandler=callback;}},response:{use:callback=>{record('register:response');responseHandler=callback;}}},request:async config=>{record('retry',config);return {data:{code:0,msg:'retried'},config};}};
const compile=ts.transpileModule(declarations(file)+'\n'+helper,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const api=new vm.Script(compile+'\ncreateAPI').runInNewContext({exports:{},Buffer,Date,AxiosHeaders,qs:dependency('qs'),hmac:dependency('@noble/hashes/hmac.js').hmac,sha256:dependency('@noble/hashes/sha2.js').sha256,utf8ToBytes:dependency('@noble/hashes/utils.js').utf8ToBytes,BASE_URL:{livenet:'https://omni.apex.exchange/api',testnet:'https://testnet.omni.apex.exchange/api'},ENDPOINTS:{KLINE:'/v3/klines',CREATE_ORDER:'/v3/order'},RECV_WINDOW:20000,virtualClock:{getCurrentTimeMs:()=>1700001234567},networkErrorOnRead:()=>()=>false,
 safeCexAxiosCreate:(config,options,exchange)=>{record('create',config.baseURL,config.timeout,config.headers,config.paramsSerializer.serialize({a:0,z:false}));return xhr;},retry:(_,config)=>record('configure:retry',config.retries),installTransportAdapter:(_,options,name)=>record('install:transport',name),appendHttpFailure:(message,response)=>{return typeof response.status==='number'&&response.status>=400&&!message.includes('HTTP '+response.status)?message+' (HTTP '+response.status+')':message;},
});
function native(v){if(v===undefined)return {kind:'undefined'};if(v===null)return {kind:'null'};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {kind:'number',bits:Number.isNaN(v)?'7ff8000000000000':b.toString('hex')};}if(Array.isArray(v))return {kind:'array',values:v.map(native)};if(typeof v==='object')return {kind:'object',values:Object.entries(v).map(([k,v])=>[k,native(v)])};return {kind:typeof v,value:v};}
const opts={key:'key',secret:'secret-é',passphrase:'pass'},exchange={name:'apexomni',emitAndLogError:message=>record('error',message)};
const configs=[
 {method:'get',url:'/v3/account',params:{symbol:'BTC-USDC',zero:0,false:false,nil:null,empty:''},headers:{'X-Trace':'trace'}},
 {method:'delete',url:'/v3/open-orders',params:{symbol:'BTC-USDC',zero:0,false:false}},
 {method:'post',url:'/v3/order',data:{symbol:'BTC-USDC',zero:0,false:false,size:'0.003'}},
 {method:'post',url:'/v3/order',data:'a=0&side=BUY',__apexUseLegacySigning:true},
 {method:'get',url:'/v3/klines',params:{symbol:'BTC-USDC'},headers:{'APEX-API-KEY':'old'}},
 {method:'GET',url:'/v3/klines',params:{symbol:'BTC-USDC'}},
 {method:'post',url:'/v3/set-initial-margin-rate',data:{initialMarginRate:'0.02500'}},
 {url:'/v3/account',params:undefined},
 {method:'get',url:'/v3/account',params:{a:0,z:false},__apexUseLegacySigning:true},
 {method:'post',url:'/v3/order',data:'z=false&a=0'},
 {method:'get',url:'/v3/account',params:{a:0},headers:{'apex-signature':false}},
 {method:'post',url:'/v3/order',data:{a:0,z:false},__apexUseLegacySigning:true},
];
async function main(){const rows=[];for(let variant=0;variant<3;variant++){ops=[];api({...opts,testnet:variant===1,tealstreetTunnel:variant===2,extra:variant===2?{recvWindow:0}:undefined},exchange);rows.push({kind:'factory',variant,ops:native(ops)});}
api(opts,exchange);
for(const config of configs){ops=[];let result;try{result={ok:true,value:native(requestHandler(config))};}catch(error){result={ok:false,message:error.message,name:error.name};}rows.push({kind:'request',input:native(config),ops:native(ops),result});}
for(const [code,msg,retried,status] of [[0,'ok',false,200],['0','body error',false,200],[20016,'signature',false,401],[10002,'bad SIGN check',false,200],[401,'unauthorized',false,401],[20016,'signature',true,401],[10001,'limit',false,429],[false,'ignored',false,500],[null,'ignored',false,200]]){const response={data:{code,msg},config:{method:'get',url:'/v3/account',params:{a:0},headers:{'X-Trace':'trace'},__apexLegacyRetried:retried},status};ops=[];let result;try{result={ok:true,value:native(await responseHandler(response))};}catch(error){result={ok:false,message:error.message,name:error.name};}rows.push({kind:'response',input:native(response),ops:native(ops),result});}
const nullResponse={data:null,config:{},status:200};let result;ops=[];try{result={ok:true,value:native(await responseHandler(nullResponse))};}catch(error){result={ok:false,message:error.message,name:error.name};}rows.push({kind:'response',input:native(nullResponse),ops:native(ops),result});
for(const config of [undefined,null]){const response={data:{code:20016,msg:'signature'},config,status:401};ops=[];let result;try{result={ok:true,value:native(await responseHandler(response))};}catch(error){result={ok:false,message:error.message,name:error.name};}assert.equal(result.ok,false,'nullish retry config throws');assert.equal(result.name,'TypeError','nullish retry config error type');rows.push({kind:'response',input:native(response),ops:native(ops),result});}
assert.equal(rows.length,27,'API source floor');assert.equal(rows.filter(r=>r.kind==='request').length,12,'request floor');for(const row of rows.filter(r=>r.kind==='request'))assert(row.result.ok,'request source baseline succeeds: '+JSON.stringify(row.result));assert.equal(rows.filter(r=>r.kind==='response').length,12,'response floor');console.log(JSON.stringify({rows,header:{node:process.version,nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH}}));}
main().catch(e=>{console.error(e);process.exitCode=1;});
