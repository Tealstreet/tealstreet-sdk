const assert=require('node:assert/strict');
// Execute the real page body and real bill mapper; capture before JSON erases numbers.
const encode=v=>v===undefined?{$undefined:true}:typeof v==='number'?{$number:Object.is(v,-0)?'-0':String(v),bits:(()=>{const b=Buffer.alloc(8);b.writeDoubleBE(v);return Number.isNaN(v)?'7ff8000000000000':b.toString('hex');})()}:typeof v==='function'?{$function:String(v)}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)])):v;
if(process.argv.includes('--numeric-probe')){
 const inputs={negativeNaN:Buffer.from('fff8000000000000','hex').readDoubleBE(),positiveNaN:NaN,negativeZero:-0,positiveZero:0,finite:1.25,negativeFinite:-2.5,positiveInfinity:Infinity,negativeInfinity:-Infinity};
 process.stdout.write(JSON.stringify(encode(inputs))+'\n');process.exit(0);
}
const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'../../../node_modules/typescript');
const root=path.resolve(__dirname,'../../..');
const read=file=>ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);
const ast=read('packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitgetExchange');
const member=klass.members.find(n=>n.name?.getText(ast)==='_fetchBillPage');
const constant=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.text==='BILL_PAGE_LIMIT'));
const protocol=read('packages/safe-cex/src/exchanges/bitget/bitget.types.ts');
const endpoints=protocol.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.text==='ENDPOINTS'));
if(!member||!constant||!endpoints)throw Error('missing bill page source declaration');
const income=read('packages/safe-cex/src/exchanges/bitget/bitget.income.ts');
const body=income.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(income).replace(/^export\s+/,'' )).join('\n');
const code=ts.transpileModule(body+'\n'+constant.getText(ast)+'\n'+endpoints.getText(protocol).replace(/^export\s+/,'')+'\nclass Source{'+member.getText(ast)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const Source=new Function(code+'\nreturn Source;')();
const values=[undefined,null,false,true,0,-0,1,-1,100,101,1e21,Infinity,-Infinity,'','00000','0','-0','1e309','junk','\ufeff12\ufeff','\u008512',[],[1],{}, {toString:1}];
const bill={cTime:'1700000000123',businessType:'close_short',billId:'b1',coin:'USDT',symbol:'BTCUSDT',amount:'12.5',fee:'-0.2'};
(async()=>{
 const cases=[];
 async function capture(name,{opts,data={code:'00000',data:{bills:[bill]}},publicOnly=false,marketId,fail,product='usdt-futures'}={}){
  const calls=[],source=new Source();source.isPublicDataOnly=publicOnly;source.apiProductType=product;
  const failure=Object.assign(new Error('original read failure'),{name:'AxiosError'});
  source._resolveMarket=symbol=>{calls.push({resolve:encode(symbol)});if(fail==='resolve')throw failure;return marketId===undefined?undefined:{id:marketId};};
  source.xhr={get:async(url,{params})=>{calls.push({url,params:encode(params)});if(fail==='read')throw failure;return {data};}};
  let output;try{output={value:encode(await source._fetchBillPage(opts))};}catch(e){output={error:{name:e.name,message:e.message,sameFailure:e===failure}};}
  cases.push({name,opts:encode(opts),data:encode(data),publicOnly,marketId:encode(marketId),fail,product,calls,output});
 }
 for(let i=0;i<values.length;i++){
  await capture(`root:${i}`,{data:values[i]});
  await capture(`opts-root:${i}`,{opts:values[i]});
  await capture(`payload:${i}`,{data:{data:values[i]}});
  for(const [j,symbol] of ['BTC',{toString:1},[],0].entries())for(const fail of [undefined,'resolve'])
   await capture(`limit-before-resolver:${i}:${j}:${fail}`,{opts:{limit:values[i],symbol},fail});
 }
 for(let i=0;i<values.length;i++)for(const field of ['limit','symbol','startTime','endTime'])await capture(`option:${field}:${i}`,{opts:{[field]:values[i]}});
 for(let i=0;i<values.length;i++)await capture(`resolved-id:${i}`,{opts:{symbol:'BTC'},marketId:values[i]});
 for(let i=0;i<values.length;i++)for(let j=0;j<values.length;j++)await capture(`code-msg:${i}:${j}`,{data:{code:values[i],msg:values[j],data:{bills:[]}}});
 for(let i=0;i<values.length;i++)await capture(`rows:${i}`,{data:{data:{bills:values[i]}}});
 for(const fail of ['read','resolve'])await capture(`failure:${fail}`,{opts:{symbol:'BTC'},fail});
 await capture('public-guard-before-coercion',{publicOnly:true,opts:{limit:{toString:1},symbol:'BTC'},fail:'read'});
 for(const product of ['usdt-futures','usdc-futures','coin-futures','susdt-futures'])await capture(`product:${product}`,{product,opts:{limit:999,symbol:'BTC',startTime:1,endTime:2}});
 await capture('mapper-throw-not-empty',{data:{data:{bills:[bill,{...bill,coin:{toString:1}},{...bill,billId:'later'}]}}});
 await capture('mapper-native-output',{data:{data:{bills:[{...bill,amount:'1e308',fee:'1e308',businessType:'unknown'},{...bill,fee:'-0'}]}}});
 for(const key of Object.getOwnPropertyNames(Object.prototype))await capture(`inherited-then-null:${key}`,{data:{data:{bills:[{...bill,businessType:key},null]}}});
 for(const key of Object.getOwnPropertyNames(Object.prototype))await capture(`inherited-category:${key}`,{data:{data:{bills:[{...bill,businessType:key}]}}});
 const utf16=[];for(const text of ['\ud800','x\udc00']){const source=new Source();source.apiProductType='usdt-futures';source._resolveMarket=()=>undefined;source.xhr={get:async()=>({data:{data:{bills:[{...bill,billId:text}]}}})};utf16.push({json:JSON.stringify({...bill,billId:text}),output:JSON.stringify(await source._fetchBillPage())});}
 if(process.argv.includes('--finding-policy')) {
  const rows=[];
  for(const billId of ['\ud800','x\udc00']) {
   const source=new Source();source.apiProductType='usdt-futures';source._resolveMarket=()=>undefined;let requests=0;
   source.xhr={get:async()=>{requests++;return {data:{data:{bills:[{...bill,billId}]}}};}};
   const records=await source._fetchBillPage();assert.equal(records.length,2,'page returns both PnL and commission');
   const units=Array.from({length:records[0].tranId.length},(_,i)=>records[0].tranId.charCodeAt(i));
   assert.equal(units[billId.length-1],billId==='\ud800'?0xd800:0xdc00,'source page native result unit BEFORE serialization');
   assert.equal(requests,1);rows.push({escaped:JSON.stringify({...bill,billId}),unit:units[billId.length-1],requests});
  }
  assert.equal(rows.length,2,'independent page native string floor');process.stdout.write(JSON.stringify({utf16:rows}));return;
 }
 const aliasSource=new Source(),aliasOpts={startTime:{tag:'before'}};let aliasParams;
 aliasSource.apiProductType='usdt-futures';aliasSource.xhr={get:async(_url,{params})=>{aliasParams=params;return {data:{data:{bills:[]}}};}};
 await aliasSource._fetchBillPage(aliasOpts);aliasOpts.startTime.tag='after-input';const afterInput=encode(aliasParams.startTime);
 aliasParams.startTime.tag='after-output';const afterOutput=encode(aliasOpts.startTime);
 process.stdout.write(JSON.stringify({"capturePolicy":{"nanBits":"7ff8000000000000","otherBits":"exact, including signed zero","witness":"CI run 38345331756 job 115125597863 Linux x86-64 Node 22.23.1: 18 params.limit NaNs fff8000000000000; frozen macOS arm64 capture: 7ff8000000000000"},cases,utf16,alias:{afterInput,afterOutput}})+'\n');
})().catch(e=>{console.error(e);process.exitCode=1;});
