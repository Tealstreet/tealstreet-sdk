// Exact source serializer expressions, not Axios form-body rendering.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const resolver=require('node:module').createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'));
const qs=resolver('qs'),qsPackage=resolver('qs/package.json');
const root=path.resolve(__dirname,'../../..');
const inputs=[{limit:-Infinity,startAt:NaN,endAt:Infinity,nil:null,absent:undefined,list:[NaN,Infinity,-Infinity,null,undefined,'a b']},{list:[],empty:{},flag:false,zero:-0,textZero:"-0",textOverflow:"1e400"},{list:[[1,undefined],[null,2]],nested:{a:NaN,b:undefined,c:null}}];
const tag=value=>value===undefined?{$undefined:true}:typeof value==='number'?{$number:Object.is(value,-0)?'-0':String(value)}:Array.isArray(value)?value.map(tag):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[k,tag(v)])):value;
const rows=[];
for(const file of ['packages/safe-cex/src/exchanges/kucoin/kucoinspot.api.ts','packages/safe-cex/src/exchanges/binance/binancespot.api.ts']){
 const text=fs.readFileSync(path.join(root,file),'utf8'),ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);let expression;
 function visit(n){if(ts.isPropertyAssignment(n)&&n.name.getText(ast)==='serialize'&&n.initializer.getText(ast).includes('qs.stringify'))expression??=n.initializer.getText(ast);ts.forEachChild(n,visit);}visit(ast);
 if(!expression)throw Error('missing actual source query serializer');
 const mod={exports:{}};new Function('module','qs',ts.transpileModule('module.exports='+expression,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(mod,qs);
 for(const input of inputs)rows.push({file,renderer:'qs',captureArchitecture:process.arch,version:qsPackage.version,fingerprint:crypto.createHash('sha256').update(fs.readFileSync(resolver.resolve('qs'))).digest('hex'),expression,input:tag(input),output:mod.exports(process.env.P4_QUERY_CONTROL==='json'?JSON.parse(JSON.stringify(input)):input)});
}
console.log(JSON.stringify(rows));
