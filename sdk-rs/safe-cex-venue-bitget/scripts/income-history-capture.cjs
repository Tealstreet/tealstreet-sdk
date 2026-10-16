// Execute the source catch boundary, including failures inside that catch.
const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'../../../node_modules/typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitgetExchange');
const member=klass.members.find(n=>n.name?.getText(ast)==='fetchIncomeHistory');
if(!member)throw Error('missing fetchIncomeHistory source');
const code=ts.transpileModule('class Source{'+member.getText(ast)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const Source=new Function(code+';return Source;')();
const encode=v=>v===undefined?{$undefined:true}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)])):v;
(async()=>{
 const cases=[];
 const values=[undefined,null,false,true,0,-1,'','failure','\ufefffailure\ufeff','\u0085failure','Σİſ😀\0',[],['x'],[null,1],{}, {toString:1},{toString:null}, {valueOf:1}, {message:'nested'}];
 async function capture(name,thrown,mode='reject',emitThrows=false){
  const calls=[],source=new Source(),opts={symbol:'BTC'},rows=[];
  const emitFailure=Object.assign(new Error('listener failure'),{name:'ListenerError'});
  source._fetchBillPage=(arg)=>{calls.push({page:true,sameOpts:arg===opts});if(mode==='throw')throw thrown;return mode==='resolve'?Promise.resolve(rows):Promise.reject(thrown);};
  source.emitAndLogError=message=>{calls.push({emit:message});if(emitThrows)throw emitFailure;};
  let output;try{const result=await source.fetchIncomeHistory(opts);output={value:result,sameRows:result===rows};}catch(e){output={error:{name:e.name,message:e.message,sameEmit:e===emitFailure}};}
  cases.push({name,thrown:encode(thrown),mode,emitThrows,calls,output});
 }
 for(let i=0;i<values.length;i++)for(const mode of ['throw','reject'])for(const emitThrows of [false,true]){
  await capture(`root:${i}:${mode}:${emitThrows}`,values[i],mode,emitThrows);
  await capture(`message:${i}:${mode}:${emitThrows}`,{message:values[i]},mode,emitThrows);
 }
 await capture('success-no-error-report',null,'resolve',true);
 // Nullish fallback must not coerce a throwing root when its message is present.
 for(const message of ['',false,0,[],null,undefined])await capture(`root-poison-message:${String(message)}`,{message,toString:1});
 const successCases=[];
 const numeric=[0,-0,NaN,Infinity,-Infinity,1e308];
 const tagged=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return {$number:String(n),bits:b.toString('hex')};};
 for(let i=0;i<numeric.length;i++){
  const rows=[{symbol:'Z',incomeType:'funding',income:numeric[i],asset:'USDT',timestamp:numeric[(i+1)%numeric.length],tranId:'z'}, {symbol:'A',incomeType:'fee',income:7,asset:'BGB',timestamp:3,tranId:'a'}];
  const source=new Source(),calls=[];source._fetchBillPage=async()=>{calls.push('page');return rows;};source.emitAndLogError=()=>{calls.push('emit');throw Error('success must not report');};
  const result=await source.fetchIncomeHistory();
  successCases.push({name:`populated:${i}`,index:i,calls,sameRows:result===rows,rows:result.map(r=>({...r,income:tagged(r.income),timestamp:tagged(r.timestamp)}))});
 }
 const utf16=[];for(const value of ['\ud800','x\udc00']){const source=new Source();let message;source._fetchBillPage=async()=>{throw{message:value};};source.emitAndLogError=v=>{message=v;};await source.fetchIncomeHistory();utf16.push({json:JSON.stringify({message:value}),emitted:JSON.stringify(message)});}
 process.stdout.write(JSON.stringify({cases,successCases,utf16})+'\n');
})().catch(e=>{console.error(e);process.exitCode=1;});
