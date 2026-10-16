const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..'),modules=new Map();
function load(file){
 if(modules.has(file))return modules.get(file);
 const exports={};modules.set(file,exports);
 vm.runInNewContext(ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require(spec){
  assert(spec.startsWith('.')||spec.startsWith('@tealstreet/'),spec);
  const target=spec.startsWith('.')?path.resolve(path.dirname(file),spec):path.join(root,'packages',spec.slice('@tealstreet/'.length));
  return load(fs.existsSync(target+'.ts')?target+'.ts':path.join(target,'index.ts'));
 }},{filename:file});return exports;
}
const enums=load(path.join(root,'packages/safe-cex/src/types.ts'));
const params=load(path.join(root,'packages/safe-cex/src/exchanges/bybit/bybit.order-params.ts'));
const omit=require(require.resolve('lodash/omit',{paths:[root,path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'../..')]}));
function subject(family){
 const [file,name]=({linear:['bybit.exchange.ts','BybitExchange'],inverse:['bybitinverse.exchange.ts','BybitInverseExchange'],spot:['bybitspot.exchange.ts','BybitSpotExchange']})[family];
 const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/bybit',file),'utf8');
 const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true),cls=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text===name);assert(cls);
 const names=['reinitOnThrow','getOrderExecutionParams','getOrderPositionIdx','getStopOrderPositionIdx',...(family==='inverse'?['getExecutionQueryWindows','EXECUTION_QUERY_MAX_RANGE_MS']:[])];
 const body=names.map(n=>{const found=cls.members.filter(m=>m.name?.getText(ast)===n);assert.equal(found.length,1,`${name}.${n}`);return found[0].getText(ast);}).join('\n');
 const context=vm.createContext({OrderSide:enums.OrderSide,PositionMode:enums.PositionMode,omit,getBybitOrderExecutionParams:params.getBybitOrderExecutionParams});
 vm.runInContext(ts.transpileModule(`class Subject { ${body} } globalThis.subject=new Subject();`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 return context.subject;
}
function observe(value){
 if(value===undefined)return {undefined:true};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {number:Number.isNaN(value)?'nan':b.toString('hex')};}
 if(Array.isArray(value))return Array.from(value,observe);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
 return value;
}
const number=token=>token==='undefined'?undefined:Number(token);
const makeError=(name,message)=>Object.assign(new Error(message),{name,marker:37});
const errorResult=(error,original)=>({error:error.name,message:error.message,marker:error.marker??null,sameError:error===original});
async function main(){
 const windows=[],inverse=subject('inverse'),defaultMax=inverse.EXECUTION_QUERY_MAX_RANGE_MS;
 const pairs=[['undefined','undefined'],['0','undefined'],['undefined','0'],['-0','0'],['0','-0'],['10','0'],['0','0'],['0','5'],['0','10'],['0','11'],['0','12'],['0','22'],['0','23'],['NaN','0'],['0','NaN'],['NaN','NaN'],['-10','10']];
 for(const [from,to]of pairs){inverse.EXECUTION_QUERY_MAX_RANGE_MS=10;windows.push({from,to,max:'10',result:observe(inverse.getExecutionQueryWindows(number(from),number(to)))});}
 for(const delta of [1,2]){inverse.EXECUTION_QUERY_MAX_RANGE_MS=defaultMax;const to=String(defaultMax+delta);windows.push({from:'0',to,max:String(defaultMax),result:observe(inverse.getExecutionQueryWindows(0,Number(to)))});}
 inverse.EXECUTION_QUERY_MAX_RANGE_MS=NaN;windows.push({from:'0',to:'5',max:'NaN',result:observe(inverse.getExecutionQueryWindows(0,5))});
 assert.equal(windows.length,20,'GUARD independent window floor');
 const reinit=[];
 for(const family of ['inverse','spot'])for(const initially of [false,true])for(const reject of [false,true]){
  const api=subject(family);api.needsReinit=initially;let resolve,rejection;
  const original={marker:73},error=makeError('RemoteError','request failed');
  const request=new Promise((ok,fail)=>{resolve=ok;rejection=fail;});
  const result=api.reinitOnThrow(request),before=api.needsReinit;
  reject?rejection(error):resolve(original);const settled=api.needsReinit;
  let outcome;try{const value=await result;outcome={value:observe(value),sameValue:value===original};}catch(e){outcome=errorResult(e,error);}
  reinit.push({family,initially,reject,before,settled,after:api.needsReinit,outcome});
 }
 assert.equal(reinit.length,8,'GUARD independent rejection floor');
 const index=[];
 const indexInputs=[];
 for(const side of ['buy','sell'])for(const reduceOnly of [undefined,false,true])for(const mode of [undefined,null,'oneway','hedged'])indexInputs.push({opts:{symbol:'BTCUSD',side,...(reduceOnly===undefined?{}:{reduceOnly})},mode});
 indexInputs.push({opts:{symbol:'BTCUSD',side:'buy',reduceOnly:false},mode:'hedged',after:{side:'sell',reduceOnly:true}});
 indexInputs.push({opts:{symbol:'BTCUSD',side:'buy',reduceOnly:true},mode:'hedged',after:{side:'sell',reduceOnly:false}});
 indexInputs.push({opts:{symbol:'BTCUSD',side:'buy',reduceOnly:'yes'},mode:true});
 indexInputs.push({opts:{side:'other',reduceOnly:[]},mode:0});
 indexInputs.push({opts:{symbol:'BTCUSD',side:'buy',reduceOnly:true},mode:'oneway',after:{side:'sell'},fail:true});
 assert.equal(indexInputs.length,29,'GUARD independent index input floor');
 for(const family of ['linear','inverse','spot'])for(const input of indexInputs){
  const api=subject(family),opts=structuredClone(input.opts),calls=[],original=makeError('ModeError','mode request failed');
  api.fetchPositionMode=symbol=>{calls.push(observe(symbol));return new Promise((ok,fail)=>{Object.assign(opts,input.after||{});input.fail?fail(original):ok(input.mode);});};
  let result;try{result={value:observe(await api.getOrderPositionIdx(opts))};}catch(e){result=errorResult(e,original);}
  index.push({family,input:observe(input),opts:input.opts,modeRaw:input.mode===undefined?null:JSON.stringify(input.mode),after:input.after||{},fail:input.fail||false,calls,result});
 }
 assert.equal(index.length,87,'GUARD independent index floor');
 const stops=[];
 for(const family of ['linear','inverse','spot'])for(const token of ['0','1','2','3','-0','NaN','Infinity','fail']){
  const api=subject(family),opts={symbol:'BTCUSD',side:'buy',reduceOnly:true,other:{tag:'nested'},remaining:9},calls=[],original=makeError('IndexError','index request failed');
  api.getOrderPositionIdx=received=>{calls.push(observe(received));return token==='fail'?Promise.reject(original):Promise.resolve(Number(token));};
  let result;try{result={value:observe(await api.getStopOrderPositionIdx(opts))};}catch(e){result=errorResult(e,original);}
  stops.push({family,opts,token,calls,result});
 }
 assert.equal(stops.length,24,'GUARD independent stop floor');
 const executions=[];
 const base={symbol:'BTCUSD',type:'limit',side:'buy',amount:3};
 const inputs=[
  {opts:base},{opts:{...base,type:'market',maxSlippagePct:1.25}},{opts:{...base,bybitBbo:{sideType:'Queue',level:2}}},
  {opts:{...base,bybitBbo:{sideType:'Counterparty',level:5}}},{opts:{...base,type:'market',bybitSlippageToleranceTicks:123}},
  {opts:{...base,maxSlippagePct:0.5}},{opts:{...base,type:'market',maxSlippagePct:11}},
  {opts:{...base,bybitBbo:{sideType:'Queue',level:2},price:77}},
  {opts:{...base,type:'market',bybitBbo:{sideType:'Queue',level:2}}},
  {opts:{...base,type:'market',bybitSlippageToleranceTicks:1.5}},
  {opts:base,categoryFail:true,errorName:'CategoryError',errorMessage:'category failed'},
  {opts:base,categoryFail:true,errorName:'CategoryError',errorMessage:''},
  {opts:{...base,maxSlippagePct:1},logFail:true},
 ];
 assert.equal(inputs.length,13,'GUARD independent execution input floor');
 for(const family of ['linear','inverse','spot'])for(const input of inputs){
  const api=subject(family),events=[],original=makeError(input.errorName||'CategoryError',input.errorMessage??'category failed');
  const logging=makeError('LogError','logging failed');
  api.getCategoryForSymbol=symbol=>{events.push({kind:'category',value:symbol});if(input.categoryFail)throw original;return family==='inverse'?'inverse':'linear';};
  api.emitAndLogError=message=>{events.push({kind:'log',value:message});if(input.logFail)throw logging;};
  let result;try{result={params:api.getOrderExecutionParams(input.opts)};}catch(e){result={...errorResult(e,input.logFail?logging:original),loggingError:e===logging};}
  executions.push({family,input,events,result});
 }
 assert.equal(executions.length,39,'GUARD independent execution floor');
 process.stdout.write(JSON.stringify({windows,reinit,index,stops,executions}));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
