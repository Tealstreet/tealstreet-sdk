const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {root,load}=require('./tables.cjs'),types=load(path.join(root,'packages/safe-cex/src/types.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts'),source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),cls=source.statements.find(ts.isClassDeclaration);
const context={store:{}};
for(const name of ['fetchPositions','fetchSymbolMaxLeverage','setLeverage','changeMarginType','changePositionMode']){
 const member=cls.members.find(m=>m.name?.getText(source)===name);if(!member)throw Error(name);
 const code=ts.transpileModule('module.exports='+member.initializer.getText(source),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;const module={exports:{}};new Function('module',...Object.keys(types),code).call(context,module,...Object.values(types));context[name]=module.exports;
}
(async()=>{
 const out=[];
 for(const c of JSON.parse(fs.readFileSync(0,'utf8'))){
  let updates=0;context.store={markets:c.markets,fetchedLeverage:c.fetchedLeverage,update(patch){updates++;Object.assign(this,patch);}};
  const positions=await context.fetchPositions(c.symbols),max=await context.fetchSymbolMaxLeverage(c.symbol);
  await context.setLeverage(c.symbol,c.leverage,c.side);await context.changeMarginType(c.isolated,c.symbol);await context.changePositionMode(c.hedged,c.symbol);
  out.push({positions,max,fetchedLeverage:context.store.fetchedLeverage,updates});
 }
 process.stdout.write(JSON.stringify(out));
})().catch(error=>{console.error(error);process.exitCode=1;});
