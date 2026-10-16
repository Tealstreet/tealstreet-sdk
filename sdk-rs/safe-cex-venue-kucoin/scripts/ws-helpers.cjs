const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=path.resolve(__dirname,'../../..'),functions={};
for(const [file,names]of [['kucoinspot.ws-public.ts',['parseNumber','normalizeTimestamp']],['kucoinspot.ws-private.ts',['normalizeMillis']]]){
 const filename=path.join(root,'packages/safe-cex/src/exchanges/kucoin',file),source=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
 for(const name of names){const decl=source.statements.filter(ts.isVariableStatement).flatMap(s=>Array.from(s.declarationList.declarations)).find(d=>d.name.getText(source)===name);if(!decl)throw Error(name);const code=ts.transpileModule('module.exports='+decl.initializer.getText(source),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;const module={exports:{}};new Function('module','Date',code)(module,{now:()=>1700000000123});functions[name]=module.exports;}
}
const {context}=require('./sizing.cjs');
const contexts={snapshotShardSymbols:{parent:context},getCandleHandlerTopic:{}};
for(const [file,name]of [['kucoin.ws-public.ts','snapshotShardSymbols'],['kucoinspot.ws-public.ts','getCandleHandlerTopic']]){
 const filename=path.join(root,'packages/safe-cex/src/exchanges/kucoin',file),source=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
 const member=source.statements.find(ts.isClassDeclaration).members.find(m=>m.name?.getText(source)===name);
 const code=ts.transpileModule('module.exports='+member.initializer.getText(source),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const module={exports:{}};new Function('module',code).call(contexts[name],module);functions[name]=module.exports;
}
function tagged(n){return typeof n==='number'?(Number.isNaN(n)?'NaN':n===Infinity?'Infinity':n===-Infinity?'-Infinity':Object.is(n,-0)?'-0':n):n??null;}
const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoin.ws-public.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const role=source.statements.find(s=>ts.isTypeAliasDeclaration(s)&&s.name.text==='KucoinPublicRole');
if(!role||!ts.isUnionTypeNode(role.type)||!role.type.types.every(t=>ts.isLiteralTypeNode(t)&&ts.isStringLiteral(t.literal)))throw Error('KucoinPublicRole stopped being a closed string union');
const roles=role.type.types.map(t=>t.literal.text);
const cases=JSON.parse(fs.readFileSync(0,'utf8'));
process.stdout.write(JSON.stringify({roles,cases:cases.map(c=>{
 context.store={markets:c.markets||[],marketsBySymbol:Object.fromEntries((c.markets||[]).map(m=>[m.symbol,m]))};
 contexts.snapshotShardSymbols.shardCount=c.shardCount;contexts.snapshotShardSymbols.shardIndex=c.shardIndex;
 return Object.fromEntries(Object.entries(functions).map(([name,fn])=>{try{return[name,{ok:tagged(fn(name==='getCandleHandlerTopic'?c.topic:c.value))}]}catch(error){return[name,{error:String(error)}]}}));
})}));
