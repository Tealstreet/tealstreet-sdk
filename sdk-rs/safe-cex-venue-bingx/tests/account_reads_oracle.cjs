const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
const file=path.join(root,'packages/safe-cex/src/exchanges/bingx/bingx.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const classes=source.statements.filter(s=>ts.isClassDeclaration(s)&&s.name?.text==='BingxExchange');assert.equal(classes.length,1);
const declarations=['fetchPrimaryBalanceData','getAccount'].map(name=>{
 const fields=classes[0].members.filter(m=>ts.isPropertyDeclaration(m)&&m.name?.getText(source)===name);assert.equal(fields.length,1);
 return `this.${name}=${fields[0].initializer.getText(source)};`;
});
const typeFile=path.join(root,'packages/safe-cex/src/exchanges/bingx/bingx.types.ts');
const typeSource=ts.createSourceFile(typeFile,fs.readFileSync(typeFile,'utf8'),ts.ScriptTarget.Latest,true);
const endpoints=typeSource.statements.filter(ts.isVariableStatement).flatMap(s=>Array.from(s.declarationList.declarations)).filter(d=>d.name.getText(typeSource)==='ENDPOINTS');assert.equal(endpoints.length,1);
const compiled=ts.transpileModule(`const ENDPOINTS=${endpoints[0].initializer.getText(typeSource)}; (function(){${declarations.join('\n')}return this;}).call({})`,{reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}});
assert(!compiled.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error));
const helper=vm.runInNewContext(compiled.outputText,{}, {filename:file});
const row=(asset,id)=>({asset,userId:id,balance:'11.25',equity:'12.5',unrealizedProfit:'1.25',realisedProfit:'2.75',availableMargin:'3.5',usedMargin:'7.75',freezedMargin:'0.125'});
const inputs=[];
for(const rows of [[],[row('USDC','a')],[row('USDC','a'),row('USDT','b')],[row('USDT','a'),row('USDT','b')],[row('usdt','a'),row('USD','b')],[row('USDT','first'),null],[null,row('USDT','b')]])inputs.push({code:0,msg:'',data:rows});
for(const id of ['', 'masked-123', 'é🦀', 0, -0, 9007199254740992, null, true, ['x','y'], {x:1}])inputs.push({code:0,msg:'',data:[row('USDT',id)]});
inputs.push({code:0,msg:'',data:[{asset:'USDT'}]}, {code:0}, {code:0,data:null}, {code:0,data:{}}, null);
for(const code of [1,-1,'0',null])for(const msg of ['', 'denied'])inputs.push({code,msg,data:[]});
inputs.push({data:[]}, {code:0,data:[row('USDT',{toString:0})]}, {code:1,msg:{toString:0}});
(async()=>{
 const cases=[];
 for(const method of ['fetchPrimaryBalanceData','getAccount'])for(const input of [...inputs,{transportError:true}]){
  const calls=[];
  helper.xhr={get:async(...args)=>{calls.push(args);if(input?.transportError){const e=new Error('network preserved');e.name='NetworkError';e.marker='identity';throw e;}return {data:input};}};
  let expected;
  try{expected={ok:await helper[method]()};}catch(e){expected={error:{name:e.name,message:e.message,marker:e.marker}};}
  cases.push({method,input,calls,expected});
 }
 const numeric=[];
 for(const method of ['fetchPrimaryBalanceData','getAccount'])for(const lexeme of ['-0','0','9007199254740993','1e400','-1e-400']) {
  const raw=JSON.stringify({code:0,data:[row('USDT','placeholder')]}).replace('"placeholder"',lexeme);
  const input=JSON.parse(raw),calls=[];
  helper.xhr={get:async(...args)=>{calls.push(args);return {data:input};}};
  const output=await helper[method]();
  let bits=null;
  if(method==='fetchPrimaryBalanceData'){const b=Buffer.alloc(8);b.writeDoubleBE(output.userId);bits=b.toString('hex');}
  numeric.push({method,raw,calls,output,bits});
 }
 const boundary=[];
 for(const method of ['fetchPrimaryBalanceData','getAccount']) {
  const input={code:0,data:[row('USDT','a\ud800b')]};
  helper.xhr={get:async()=>({data:input})};
  const result=await helper[method]();
  boundary.push({method,raw:JSON.stringify(input),units:Array.from({length:result.userId.length},(_,i)=>result.userId.charCodeAt(i))});
 }
 process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},cases,boundary,numeric}));
})().catch(e=>{console.error(e);process.exitCode=1;});
