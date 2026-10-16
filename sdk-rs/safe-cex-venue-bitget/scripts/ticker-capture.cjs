// Execute both actual class members, keeping numeric bits before JSON transport.
const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'../../../node_modules/typescript');
const root=path.resolve(__dirname,'../../..');
const encode=v=>v===undefined?{$undefined:true}:typeof v==='number'?{$number:Object.is(v,-0)?'-0':String(v)}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)])):v;
const values=[undefined,null,false,true,0,-0,1,-1,Number.MAX_VALUE,-Number.MAX_VALUE,Number.MIN_VALUE,Infinity,-Infinity,NaN,'',' ','\ufeff','\u0085','\ufeff12.5','\u008512.5','0','-0','-0.0','12junk','0x10','Infinity','-Infinity','NaN','1e400','1e306','-1e306',[],[1],{}, {toString:1},'İ','ſ','ΟΣ','𐐀','\0'];
const fields=[['bidPr','bidPrice','bid'],['askPr','askPrice','ask'],['lastPr','lastPrice','last'],['markPrice','markPr','mark'],['indexPrice','indexPr','index'],['change24h','changeUtc24h','change'],['fundingRate'],['nextFundingTime','fundingTime'],['baseVolume','volume'],['quoteVolume']];
const result=[];
for(const [file,name] of [['bitget.ws-public.ts','BitgetPublicWebsocket'],['bitget.ws-secondary-public.ts','BitgetSecondaryPublicWebsocket']]){
 const text=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/bitget',file),'utf8');
 const ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
 const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===name);
 const members=['toFiniteNumber','buildTickerUpdate'].map(name=>{const member=klass.members.find(n=>n.name?.getText(ast)===name);if(!member)throw Error('missing ticker member '+name);return member.getText(ast);});
 const compiled=ts.transpileModule('class Source {'+members.join('\n')+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
 const Source=new Function(compiled+'\nreturn Source;')();const source=new Source();
 const scalar=values.map((input,i)=>({name:`scalar:${i}`,input:encode(input),output:encode(source.toFiniteNumber(input))}));
 const cases=[];
 function capture(name,input){let output;try{output={value:encode(source.buildTickerUpdate(input))};}catch(e){output={error:{name:e.name,message:e.message}};}cases.push({name,input:encode(input),output});}
 for(let i=0;i<values.length;i++)capture(`root:${i}`,values[i]);
 for(const keys of fields){
  for(let i=0;i<values.length;i++)for(let at=0;at<keys.length;at++)capture(`field:${keys[0]}:${at}:${i}`,Object.fromEntries(keys.map((k,j)=>[k,j<at?null:j===at?values[i]:'91'])));
  if(keys.length>1)for(let a=0;a<values.length;a++)for(const b of [undefined,null,0,-0,'7',[]])capture(`conflict:${keys[0]}:${a}:${String(b)}:${Object.is(b,-0)}`,{[keys[0]]:values[a],[keys[1]]:b,...(keys[2]?{[keys[2]]:'9'}:{})});
 }
 capture('all-output-order',Object.fromEntries(fields.map((keys,i)=>[keys[0],i+1])));
 capture('derived-positive-infinity',{change24h:Number.MAX_VALUE});
 capture('derived-negative-infinity',{change24h:-Number.MAX_VALUE});
 capture('signed-zero-all-fields',Object.fromEntries(fields.map(keys=>[keys[0],-0])));
 const utf16=['\ud800','12\udc00'].map(value=>({json:JSON.stringify(value),scalar:encode(source.toFiniteNumber(value)),ticker:encode(source.buildTickerUpdate({bidPr:value}))}));
 const excluded=cases.filter(c=>JSON.stringify(c.input).includes('"$number":"NaN"'));
 result.push({file,scalar,cases:cases.filter(c=>!excluded.includes(c)),excluded,utf16});
}
process.stdout.write(JSON.stringify(result)+'\n');
