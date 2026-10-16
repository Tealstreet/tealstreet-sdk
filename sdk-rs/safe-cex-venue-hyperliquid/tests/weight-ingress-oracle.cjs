const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const base=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/hyperliquid');
function load(file){const text=fs.readFileSync(file,'utf8'),code=ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,exports={};vm.runInNewContext(code+(file.endsWith('hyperliquid.rateLimiter.ts')?'\nexports.isEndpoint=isEndpoint;exports.parseRequestData=parseRequestData;exports.getExchangeBatchLength=getExchangeBatchLength;exports.getInfoExtraWeight=getInfoExtraWeight;':''),{exports,require:name=>{assert.equal(name,'./hyperliquid.types');return load(path.join(base,'hyperliquid.types.ts'))},Set,Promise,Date,setTimeout},{filename:file});return exports}
const rate=load(path.join(base,'hyperliquid.rateLimiter.ts'));

const inputs=['{"toString":0}','{"valueOf":0,"toString":0}','[{"toString":0}]','{"toString":"x"}','{}','[60]'].map(raw=>({url:'/info',data:{type:'candleSnapshot',limit:JSON.parse(raw)}}));
for(const type of ['unexpected',undefined,null]){const action={'':Array(40).fill(null)};if(type!==undefined)action.type=type;inputs.push({url:'/exchange',data:{action}})}
inputs.push({url:'/exchange',data:{action:{type:'order',orders:Array(40).fill(null)}}});
for(const unused of ['"\\ud800"','['.repeat(140)+'0'+']'.repeat(140),'"\\ud83d\\ude00"','"\\\\ud800"'])inputs.push({url:'/info',data:'{"type":"userRole","unused":'+unused+'}'});
assert.equal(inputs.length,14,'independent source weight input floor');
const expected=[null,null,null,null,20,21,1,1,1,2,60,60,60,60];let completed=0,last;
for(let index=0;index<inputs.length;index++){
 try{const actual=rate.getHyperliquidRestRequestWeight(inputs[index]);assert(index>=4,'source required Number error');assert.equal(actual,expected[index],'source native expected weight at '+index)}
 catch(error){if(index>=4)throw error;assert.equal(error.name,'TypeError','source Number error name');assert.equal(error.message,'Cannot convert object to primitive value','source Number error message')}
 completed++;last=index;
}
assert.equal(completed,14,'independent completed source weight floor');assert.equal(last,13,'source last weight case');
const original={type:'userRole'};assert.equal(rate.getHyperliquidRestRequestWeight({url:'/info',data:original}),60,'source object positive');

let prefixes=0;for(const url of ['\ud800/info','\udc00/info','😀/info','scalar/info']){assert.equal(rate.isEndpoint(url,'/info'),true,'source native UTF16 endpoint prefix');prefixes++}assert.equal(prefixes,4,'independent source endpoint prefix floor');
assert.equal(rate.parseRequestData(original),original,'source parser original object allocation');
let typeReads=0,orderReads=0;
const action={get type(){typeReads++;return 'order'},get orders(){orderReads++;return Array(40).fill(null)}};
assert.equal(rate.getExchangeBatchLength(action),40,'source accessor batch length');assert.equal(typeReads,1,'source native action type reads');assert.equal(orderReads,1,'source native action orders reads');
let conversions=0;const limit={valueOf(){conversions++;return 60}};
assert.equal(rate.getInfoExtraWeight('candleSnapshot',limit),1,'source executable Number hook weight');assert.equal(conversions,1,'source Number hook effect');
const marker={name:'NumberMarker',response:{status:429}};assert.throws(()=>rate.getInfoExtraWeight('candleSnapshot',{valueOf(){throw marker}}),error=>error===marker,'source original Number hook error');
let typeCount=0;assert.equal(rate.getHyperliquidRestRequestWeight({url:'/info',data:{get type(){return ++typeCount===1?'userRole':'userFills'},limit:60}}),63,'source repeated live data type read');assert.equal(typeCount,2,'source independent data type read count');
let urlCount=0;assert.equal(rate.getHyperliquidRestRequestWeight({get url(){return ++urlCount===1?'/other':'/info'},data:{type:'userRole'}}),60,'source repeated live url read');assert.equal(urlCount,2,'source independent URL read count');

(async()=>{
 const apiFile=path.join(base,'hyperliquid.api.ts'),ast=ts.createSourceFile(apiFile,fs.readFileSync(apiFile,'utf8'),99,true);
 const declaration=ast.statements.filter(ts.isVariableStatement).flatMap(statement=>Array.from(statement.declarationList.declarations)).find(declaration=>declaration.name.getText(ast)==='addRateLimitInterceptor');assert(declaration?.initializer,'actual rate interceptor initializer');
 const actionDeclaration=ast.statements.filter(ts.isVariableStatement).flatMap(statement=>Array.from(statement.declarationList.declarations)).find(declaration=>declaration.name.getText(ast)==='getRequestActionType');assert(actionDeclaration?.initializer,'actual action-type initializer');
 const actionCode=ts.transpileModule('return ('+actionDeclaration.initializer.getText(ast)+');',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,actionType=new Function(actionCode)();
 for(const unused of ['"\\ud800"','['.repeat(140)+'0'+']'.repeat(140),'"\\ud83d\\ude00"'])assert.equal(actionType('{"action":{"type":"cancel"},"unused":'+unused+'}'),'cancel','source native action-type parser accepts valid JSON');
 const code=ts.transpileModule('return ('+declaration.initializer.getText(ast)+');',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
 const waits=[];let interceptor;
 const install=new Function('isSigningCaptureRequest','hyperliquidRestRateLimiter','getHyperliquidRestRequestWeight',code)(config=>config.capture,{async waitForToken(weight){waits.push(weight)}},rate.getHyperliquidRestRequestWeight);
 install({interceptors:{request:{use(callback){interceptor=callback}}}});
 const config={url:'/info',data:{type:'candleSnapshot',limit:{toString:0}}};
 await assert.rejects(interceptor(config),error=>error.name==='TypeError'&&error.message==='Cannot convert object to primitive value','source native API Number error before pacing');assert.equal(waits.length,0,'source independent zero limiter calls');
 const captured={...config,capture:true};assert.equal(await interceptor(captured),captured,'source capture original request identity');assert.equal(waits.length,0,'source capture no pacing');
 const positive={url:'/info',data:{type:'candleSnapshot',limit:60}};assert.equal(await interceptor(positive),positive,'source paced original request identity');assert.deepEqual(waits,[21],'source positive limiter reachability');
 console.log(JSON.stringify({count:completed,last}));
})().catch(error=>{console.error(error);process.exitCode=1});
