// Execute the actual getter initializers against the actual maintained DefaultStore.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'compiler_required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
const external=require('node:module').createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH));
const context=vm.createContext({structuredClone,process,setTimeout,clearTimeout,console});
const cache=new Map();
function execute(file) {
  if(cache.has(file))return cache.get(file);
  const exports={};cache.set(file,exports);
  const output=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
  vm.runInContext(`(function(exports,require){${output}\n})`,context,{filename:file})(exports,spec=>{
    if(spec.startsWith('@tealstreet/'))return execute(path.join(root,'packages',spec.slice('@tealstreet/'.length)+'.ts'));
    if(!spec.startsWith('.'))return external(spec);
    const target=path.resolve(path.dirname(file),spec);
    return execute(fs.existsSync(target+'.ts')?target+'.ts':path.join(target,'index.ts'));
  });return exports;
}
const {DefaultStore}=execute(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BlofinExchange');
const constants=execute(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.types.ts'));
context.ENDPOINTS=constants.ENDPOINTS;
context.roundUSD=execute(path.join(root,'packages/safe-cex/src/utils/roundUSD.ts')).roundUSD;
const factories={};
for(const name of ['fetchTickersRaw','fetchTickers','formatTickers']) {
 const member=klass.members.find(m=>m.name?.text===name);
 assert(member&&ts.isPropertyDeclaration(member)&&ts.isArrowFunction(member.initializer),'read_source_shape '+name);
 factories[name]=vm.runInContext(ts.transpileModule(`(function(){return ${member.initializer.getText(ast)};})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
}
function observe(value){
 if(value===undefined)return{undefined:true};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return{number:Number.isNaN(value)?'nan':b.toString('hex')};}
 if(Array.isArray(value))return Array.from(value,observe);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
 return value;
}
const market={id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:0.01,price:0.25},limits:{amount:{min:1,max:999},leverage:{min:2,max:17}}};
const fallback={id:'stored-id',symbol:'stored-symbol',bid:1.125,ask:2.25,last:3.375,mark:4.5,index:5.625,percentage:6.75,fundingRate:7.875,fundingTime:8.5,volume:9.125,quoteVolume:10.25,openInterest:11.375};
const stored=[fallback,{...fallback,id:'second-stored',symbol:'second-symbol',last:42.875}];
const base={instId:'BTC-USDT',open24h:'12.25',last:'13.75',bidPrice:'13.5',askPrice:'14.125',volCurrency24h:'17.375',vol24h:'29.625'};
const inputs=[
 {data:[base,{...base,last:'27.875',bidPrice:'26.125',askPrice:'28.375'}]},
 {data:[base]},{data:[]},{},{data:null},null,{data:{}},{data:false},{data:7},
 {data:[null]},{data:[{...base,instId:'missing'},base]},'text',7,
];
const fixtures=inputs.map(input=>({body:JSON.stringify(input),fail:false}));
fixtures.push({body:'{"data":[{"instId":"BTC-USDT","open24h":-0,"last":-0,"bidPrice":-0,"askPrice":1e400,"volCurrency24h":9007199254740993,"vol24h":-1e-400}]}',fail:false});
fixtures.push({fail:true});
for(const msg of ['body-message','',false,0,null,[],{},3,true])fixtures.push({body:JSON.stringify({msg}),fail:true});
fixtures.push({body:'"text"',fail:true},{body:'{}',fail:true});
assert.equal(fixtures.length,26,'READ-SOURCE fixture total');
const cases=[];
async function run(fixture,mode,method){
 const store=new DefaultStore();store.update({markets:[market],tickers:stored});
 const sentinel=new Error('host:unique'),logs=[],requests=[];
 let responseData;
 const host={store,xhr:{async get(url){requests.push({method:'GET',url});if(fixture.fail)throw {message:'network-message',...(fixture.body===undefined?{}:{response:{data:JSON.parse(fixture.body)}})};responseData=JSON.parse(fixture.body);return{data:responseData};}},
 emitAndLogError(message){logs.push(observe(message));if(mode===2)store.update({tickers:[{...fallback,id:'after-log',symbol:'after-log-symbol',last:37.125}]});if(mode===1)throw sentinel;}};
 for(const [name,factory] of Object.entries(factories))host[name]=factory.call(host);
 let result,returned;try{returned=await host[method]();result={ok:observe(returned)};}catch(error){result={error:error.name,message:error.message,identity:error===sentinel};}
 cases.push({fixture,mode,method,logs,requests,result,rawAlias:method==='fetchTickersRaw'&&!!returned&&typeof returned==='object'&&returned===responseData?.data});
}
(async()=>{
 for(const fixture of fixtures)for(let mode=0;mode<3;mode++)for(const method of ['fetchTickersRaw','fetchTickers'])await run(fixture,mode,method);
 assert.equal(cases.length,156,'READ-SOURCE corpus total');
 const originalCases=Buffer.from(JSON.stringify(cases));
 assert.equal(originalCases.length,63815,'READ original corpus bytes');
 assert.equal(require('node:crypto').createHash('sha256').update(originalCases).digest('hex'),'b6b4683cd189dd9ad65aeea74b1db365ea143c0c11a87245899a4d9bedd6fde6','READ original corpus identity');
 // Execute actual Axios transformation to witness its unparsed-text boundary.
 const axios=external('axios');
 const boundaries=['not-json','', '{"data":"\\ud800"}', '['.repeat(140)+'0'+']'.repeat(140)];
 const boundaryResults=[];
 for(const body of boundaries){
  const store=new DefaultStore();const host={store,xhr:{async get(){return{data:axios.defaults.transformResponse[0].call(axios.defaults,body)};}},emitAndLogError(){}};
  for(const [name,factory] of Object.entries(factories))host[name]=factory.call(host);
  const result=await host.fetchTickersRaw();
  if(boundaryResults.length===2){assert.equal(result.charCodeAt(0),0xd800,'READ source surrogate accepted');boundaryResults.push({codeUnit:result.charCodeAt(0)});}
  else {assert.equal(result,undefined,'READ source boundary undefined');boundaryResults.push({undefined:true});}
 }
 const phaseProbes=[];
 for(const value of [JSON.stringify({data:[base]}),{data:[base]},[1,2]]) {
  const host={xhr:{async get(){return{data:value};}},emitAndLogError(){throw new Error('READ phase must not emit');}};
  host.fetchTickersRaw=factories.fetchTickersRaw.call(host);
  phaseProbes.push({body:JSON.stringify(value),result:observe(await host.fetchTickersRaw())});
 }
 assert.equal(phaseProbes.length,3,'READ source phase total');
 process.stdout.write(JSON.stringify({phaseProbes,header:{architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),nodeVersion:process.version},market,stored,cases,boundaries,boundaryResults}));
})().catch(error=>{console.error(error);process.exitCode=1;});
