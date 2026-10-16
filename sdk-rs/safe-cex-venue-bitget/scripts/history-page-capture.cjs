// Actual private page method; xhr.get is the parsed response seam, not a fake pager.
const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'../../../node_modules/typescript');
const root=path.resolve(__dirname,'../../..');
const file=path.join(root,'packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitgetExchange');
const member=klass.members.find(n=>n.name?.getText(ast)==='_fetchFillsPage');
if(!member)throw Error('missing _fetchFillsPage');
const protocolFile=path.join(root,'packages/safe-cex/src/exchanges/bitget/bitget.types.ts');
const protocol=ts.createSourceFile(protocolFile,fs.readFileSync(protocolFile,'utf8'),ts.ScriptTarget.Latest,true);
const endpoint=protocol.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.text==='ENDPOINTS'));
if(!endpoint)throw Error('missing ENDPOINTS');
const code=ts.transpileModule(endpoint.getText(protocol).replace(/^export\s+/,'')+'\nclass Source {'+member.getText(ast)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const Source=new Function(code+'\nreturn Source;')();
const encode=v=>v===undefined?{$undefined:true}:typeof v==='number'?{$number:Object.is(v,-0)?'-0':String(v)}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)])):v;
const values=[undefined,null,false,true,0,-0,1,1e21,Infinity,-Infinity,'','00000','0','1','\ufeffbad\ufeff','\u0085bad\u0085',[],[1],{}, {toString:1}];
(async()=>{
 const cases=[];
 async function capture(name,data,params={productType:'USDT-FUTURES',limit:100},failure){
  const source=new Source(),calls=[];
  source.xhr={get:async(url,config)=>{calls.push({url,params:encode(config.params),sameParams:config.params===params});if(failure)throw failure;return {data};}};
  let output;try{const value=await source._fetchFillsPage(params);output={value:encode(value),sameRows:value.rows===data?.data?.fillList};}catch(e){output={error:{name:e.name,message:e.message,sameFailure:e===failure}};}
  cases.push({name,data:encode(data),params:encode(params),failure:failure?{name:failure.name,message:failure.message}:undefined,calls,output});
 }
 for(let i=0;i<values.length;i++)await capture(`root:${i}`,values[i]);
 for(let i=0;i<values.length;i++)for(let j=0;j<values.length;j++)await capture(`code-msg:${i}:${j}`,{code:values[i],msg:values[j],data:{fillList:[{id:'retained'}],endId:'next'}});
 for(let i=0;i<values.length;i++)await capture(`payload:${i}`,{code:'00000',data:values[i]});
 for(let i=0;i<values.length;i++)for(const rows of [undefined,null,[],[null],[{id:'x'}],[0,-0,Infinity,-Infinity,1e21],[{price:-0,qty:Infinity}],{},'wrong'])await capture(`rows-cursor:${i}:${JSON.stringify(rows)}`,{code:'00000',data:{fillList:rows,endId:values[i]}});
 await capture('transport-rejection',undefined,{symbol:'BTCUSDT',idLessThan:'cursor'},Object.assign(new Error('rate limited'),{name:'AxiosError'}));
 await capture('no-empty-on-coercion-error',{code:{toString:1},msg:{toString:1}});
 const source=new Source(),data={code:'00000',data:{fillList:[{id:'before'}],endId:'next'}};
 source.xhr={get:async()=>({data})};const result=await source._fetchFillsPage({});
 data.data.fillList[0].id='after-input';data.data.fillList.push({id:'added-input'});const afterInput=encode(result.rows);
 result.rows[0].id='after-output';result.rows.push({id:'added-output'});const afterOutput=encode(data.data.fillList);
 const utf16=[];for(const text of ['\ud800','x\udc00']){source.xhr={get:async()=>({data:{data:{fillList:[{id:text}],endId:text}}})};const output=await source._fetchFillsPage({});utf16.push({json:JSON.stringify({data:{fillList:[{id:text}],endId:text}}),rowsJson:JSON.stringify(output.rows),endIdJson:JSON.stringify(output.endId)});}
 process.stdout.write(JSON.stringify({cases,alias:{afterInput,afterOutput},utf16})+'\n');
})().catch(error=>{console.error(error);process.exitCode=1;});
