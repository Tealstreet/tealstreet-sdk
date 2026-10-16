const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {root,load} = require('./tables.cjs');
const filename = path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoin.exchange.ts');
const parsed = ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
const cls = parsed.statements.find(node=>ts.isClassDeclaration(node) && node.name.text === 'KucoinExchange');
const names = ['getOrderPlanType','getOrderSideV2','getOrderSide','getAlgoOrderSide','getPositionForOrder','getKucoinMarginMode','getKucoinPositionSide','mapStopPriceType','getKucoinStopDirection','isVirtualTPSLOrder','isAlgoOrder','parseMarginType','parsePositionMode','parseRowPositionMode'];
const types = load(path.join(root,'packages/safe-cex/src/types.ts'));
const context = {store:{positions:[]}};
for (const name of names) {
 const member = cls.members.find(member=>member.name?.getText(parsed) === name);
 if (!member?.initializer) throw new Error(`missing live helper ${name}`);
 const code = ts.transpileModule(`module.exports = ${member.initializer.getText(parsed)};`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const module={exports:{}};
 new Function('module',...Object.keys(types), code).call(context,module,...Object.values(types));
 context[name] = module.exports;
}
module.exports={context};
if(require.main === module) {
const cases = JSON.parse(fs.readFileSync(0,'utf8'));
const result = cases.map(({opts,positions,position,id,trigger,mode})=> {
 context.store.positions=positions;
 const out={};
 for (const name of names.filter(name => !name.startsWith("parse"))) {
  let args;
  if (name==='getOrderSide' || name==='getOrderSideV2') args=[opts,mode];
  else if(name==='getKucoinMarginMode') args=[position];
  else if(name==='getKucoinPositionSide') args=[opts,position];
  else if(name==='mapStopPriceType') args=[trigger];
  else if(name==='isVirtualTPSLOrder') args=[{id}];
  else args=[opts];
  try { out[name]={ok:context[name](...args) ?? null}; } catch(error) {out[name]={error:error.message};}
 }
 return out;
});
process.stdout.write(JSON.stringify(result));
}
