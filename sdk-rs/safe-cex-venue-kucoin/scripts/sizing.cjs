const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {root,load}=require('./tables.cjs');
const env={...load(path.join(root,'packages/safe-cex/src/types.ts')),...load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts')),...load(path.join(root,'packages/safe-cex/src/utils/symbolConverter.ts'))};
const context={store:{},options:{}};
for(const [venue,names] of [['kucoin',['toKucoinSymbol','toApiSymbol']]]) {
 const file=path.join(root,`packages/safe-cex/src/exchanges/kucoin/${venue}.exchange.ts`);
 const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const cls=source.statements.find(ts.isClassDeclaration);
 for(const name of names) {
  const member=cls.members.find(m=>m.name?.getText(source)===name);if(!member)throw Error(name);
  const expr=member.initializer?.getText(source) ?? `function() ${member.body.getText(source)}`;
  const code=ts.transpileModule('module.exports='+expr,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
  const module={exports:{}};new Function('module',...Object.keys(env),code).call(context,module,...Object.values(env));context[name]=module.exports;
 }
}
module.exports={context};
