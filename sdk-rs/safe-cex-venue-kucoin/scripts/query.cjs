const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const runtimeRequire=process.env.SAFE_CEX_TYPESCRIPT_PATH ? require('node:module').createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH)) : require;
const qs=runtimeRequire('qs');
const {root,load}=require('./tables.cjs');
const tables=load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts'));
function extract(venue,name) {
 const file=path.join(root,`packages/safe-cex/src/exchanges/kucoin/${venue}.api.ts`);
 const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const decl=source.statements.filter(ts.isVariableStatement).flatMap(s=>Array.from(s.declarationList.declarations)).find(d=>d.name.getText(source)===name);
 if(!decl)throw Error(`missing live helper ${name}`);
 const code=ts.transpileModule('module.exports='+decl.initializer.getText(source),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const module={exports:{}};new Function('module','qs','KUCOIN_SPOT_PUBLIC_ENDPOINTS',code)(module,qs,tables.KUCOIN_SPOT_PUBLIC_ENDPOINTS);return module.exports;
}
const stringify=extract('kucoin','stringifyQuery'),isPublic=extract('kucoinspot','isPublicEndpoint');
const cases=JSON.parse(fs.readFileSync(0,'utf8'));
process.stdout.write(JSON.stringify(cases.map(c=>{
 const value=c.special?{missing:undefined,nil:null,array:[undefined,null,NaN,Infinity,-Infinity,-0],big:BigInt('900719925474099312345')} : c.value;
 return {encoded:stringify(value,true),raw:stringify(value,false),public:isPublic(c.url)};
})));
