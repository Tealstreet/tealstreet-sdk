const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {root,load}=require('./tables.cjs');
const {context}=require('./sizing.cjs');
const env={...load(path.join(root,'packages/safe-cex/src/types.ts')),...load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts')),...load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoin.types.ts')),...load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts')),virtualClock:{getCurrentTimeMs:()=>1700000000123},Date:{now:()=>1700000000123}};
const methods={};
for(const venue of ['kucoin','kucoinspot']){
 const file=path.join(root,`packages/safe-cex/src/exchanges/kucoin/${venue}.exchange.ts`);
 const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const cls=source.statements.find(ts.isClassDeclaration);
 const method=cls.members.find(m=>m.name?.getText(source)==='mapOrder');
 const code=ts.transpileModule('module.exports='+method.initializer.getText(source),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const module={exports:{}};new Function('module',...Object.keys(env),code).call(context,module,...Object.values(env));methods[venue]=module.exports;
}
const cases=JSON.parse(fs.readFileSync(0,'utf8'));
process.stdout.write(JSON.stringify(cases.map(c=>{
 context.store={marketsBySymbol:Object.fromEntries(c.markets.map(m=>[m.symbol,m]))};
 try{const ok=methods[c.venue](c.row);if(c.inspectInherited){const types=o=>Object.fromEntries(['status','type','timeInForce'].map(k=>[k,typeof o[k]]));return {ok,$types:Array.isArray(ok)?ok.map(types):types(ok)};}return {ok};}catch(error){return {error:error.message}}
}),(_key,value)=>typeof value==='number'&&!Number.isFinite(value)?{$number:String(value)}:value));
