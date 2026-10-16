const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function parse(file){const s=ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),99,true);assert.equal(s.parseDiagnostics.length,0,'operations_source_parse');return s;}
function execute(code,globals={}){const module={exports:{}};const out=ts.transpileModule(code,{reportDiagnostics:true,compilerOptions:{module:1,target:9}});assert.equal(out.diagnostics.length,0,'operations_transpile');vm.runInNewContext(out.outputText,{module,exports:module.exports,...globals});return module.exports;}
const sdk=parse('packages/safe-cex/src/exchanges/paradex/paradex.sdk.ts');
const bodies=['getParaclearBalance','getSocializedLossFactor'].map(name=>{const nodes=sdk.statements.filter(n=>ts.isFunctionDeclaration(n)&&n.name.text===name);assert.equal(nodes.length,1,'operations_sdk_identity');return nodes[0].getText(sdk);}).join('\n');
const exchange=parse('packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts');
const cls=exchange.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='ParadexExchange');
const members=['dispose','validateTpSlPrices'].map(name=>{const nodes=cls.members.filter(n=>n.name?.getText(exchange)===name);assert.equal(nodes.length,1,'operations_exchange_identity');return nodes[0].getText(exchange);}).join('\n');
const shared=parse('packages/safe-cex/src/types.ts');
const {OrderSide}=execute(shared.statements.filter(n=>ts.isEnumDeclaration(n)&&n.name.text==='OrderSide').map(n=>n.getText(shared)).join('\n'));
const {LogSeverity}=execute(fs.readFileSync(path.join(root,'packages/logging/src/LogSeverity.ts'),'utf8'));
const {Harness}=execute(`export class Harness {${members}}`,{OrderSide,LogSeverity});
const units=value=>Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
async function drain(){for(let i=0;i<12;i++)await Promise.resolve();}
async function read(kind,mode){
 const trace=[],original={tag:'original'},config={tag:'config'},provider={tag:'provider'},address='address-\ud800',token='token-\ud800';
 let loadResolve,loadReject,readResolve,readReject;
 const loading=new Promise((r,j)=>{loadResolve=r;loadReject=j;}),reading=new Promise((r,j)=>{readResolve=r;readReject=j;});
 const value={get toString(){trace.push(['stringify-get']);if(mode==='stringify-get')throw original;return function(){trace.push(['stringify-call',this===value]);if(mode==='stringify-call')throw original;return `${kind}-\ud800`;};}};
 const account={get config(){trace.push(['config']);if(mode==='config')throw original;return config;},get provider(){trace.push(['provider']);if(mode==='provider')throw original;return provider;},get account(){trace.push(['account']);return {get address(){trace.push(['address']);if(mode==='address')throw original;return address;}};}};
 const api={};Object.defineProperty(api,kind==='balance'?'getTokenBalance':'getSocializedLossFactor',{get(){trace.push(['method',kind]);if(mode==='method')throw original;return function(request){trace.push(['call',this===api,Object.keys(request),request.config===config,request.provider===provider,...(kind==='balance'?[units(request.account),units(request.token)]:[])]);if(mode==='call-sync')throw original;return reading;};}});
 const sdkValue={get Paraclear(){trace.push(['paraclear']);return api;}};
 const functions=execute(bodies,{getParadexSDK(){trace.push(['load']);if(mode==='load-sync')throw original;return loading;}});
 const pending=functions[kind==='balance'?'getParaclearBalance':'getSocializedLossFactor'](account,token).then(value=>({ok:true,units:units(value)}),e=>({ok:false,same:e===original}));
 const before=trace.slice();if(mode==='load-async')loadReject(original);else loadResolve(sdkValue);await drain();const middle=trace.slice();
 if(mode==='call-async'&&trace.some(entry=>entry[0]==='call'))readReject(original);else readResolve(value);
 const outcome=await pending;return {kind,mode,before,middle,trace,outcome};
}
function bits(value){const b=Buffer.alloc(8);b.writeDoubleBE(value);return Number.isNaN(value)?'7ff8000000000000':b.toString('hex');}
const scalar=value=>value===undefined?null:bits(value);
const triples=[[undefined,undefined,undefined],[0,5,9],[-0,5,9],[NaN,5,9],[25,undefined,undefined],[25,30,20],[25,20,30],[25,25,undefined],[25,undefined,25],[25,undefined,30],[25,undefined,20],[25,0,-0],[25,NaN,NaN],[25,Infinity,-Infinity],[Infinity,Infinity,undefined],[-Infinity,undefined,-Infinity],[-25,-30,-20],[30.125,31.0625,29.03125],[1e21,1e21,undefined],[1e-7,undefined,1e-7]];
function validation(side,values,throwLog=false){const h=new Harness(),trace=[],error={tag:'logger'};h.log=(text,severity)=>{trace.push([text,severity]);if(throwLog)throw error;};let result;try{result={ok:true,value:h.validateTpSlPrices({side,price:values[0],takeProfit:values[1],stopLoss:values[2]})};}catch(e){result={ok:false,same:e===error};}return {side,values:values.map(scalar),throwLog,trace,result};}
(async()=>{const reads=[];for(const kind of ['balance','factor'])for(const mode of ['ok','load-sync','load-async','method','config','provider','address','call-sync','call-async','stringify-get','stringify-call'])reads.push(await read(kind,mode));
 const validations=[];for(const side of ['buy','sell'])for(const values of triples)validations.push(validation(side,values));
 for(const [side,values]of [['buy',[25,20,undefined]],['buy',[25,undefined,30]],['sell',[25,30,undefined]],['sell',[25,undefined,20]]])validations.push(validation(side,values,true));
 const disposals=[];for(const initial of [false,true])for(const mode of ['ok','public','private']){const h=new Harness(),trace=[],error={tag:'dispose'};h.isDisposed=initial;h.publicWebsocket={dispose(){trace.push(['public',h.isDisposed]);if(mode==='public')throw error;}};h.privateWebsocket={dispose(){trace.push(['private',h.isDisposed]);if(mode==='private')throw error;}};let result;try{result={ok:true,undefined:h.dispose()===undefined};}catch(e){result={ok:false,same:e===error};}disposals.push({initial,mode,trace,result,disposed:h.isDisposed});}
 process.stdout.write(JSON.stringify({header:{arch:process.arch,node:process.version,nodeMajor:Number(process.versions.node.split('.')[0])},reads,validations,disposals}));
})().catch(error=>{console.error(error);process.exitCode=1;});
