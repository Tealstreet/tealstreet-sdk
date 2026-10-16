'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const file=path.join(root,'packages/safe-cex/src/types.ts');
const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const names=['OrderSide','OrderType','OrderStatus'];
const declarations=tree.statements.filter(node=>ts.isEnumDeclaration(node)&&names.includes(node.name.text));
assert.equal(declarations.length,3);
const compile=text=>ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const enums={};vm.runInNewContext(compile(declarations.map(node=>node.getText(tree)).join('\n')),{exports:enums});
const exportsObject={};
vm.runInNewContext(compile(fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.order-history.ts'),'utf8')),{exports:exportsObject,require:name=>{assert.equal(name,'../../types');return enums;}});
const row=oid=>({order:{oid,coin:'BTC',timestamp:1,side:'B',origSz:'1',sz:'0',limitPx:'1',orderType:'limit'},status:'open'});
let completed=0;
for(const unit of [0xd800,0xdfff]) {
  const input=row(String.fromCharCode(unit));
  const mapped=exportsObject.mapHyperliquidHistoricalOrder(input,()=>undefined);
  assert.equal(typeof mapped.id,'string');assert.equal(mapped.id.length,1);assert.equal(mapped.id.charCodeAt(0),unit,'SOURCE historical returned id unit');
  const collapsed=exportsObject.collapseHyperliquidOrderHistory([input]);
  assert.equal(collapsed.length,1);assert.equal(collapsed[0],input,'SOURCE historical returned original winner');
  assert.equal(collapsed[0].order.oid.length,1);assert.equal(collapsed[0].order.oid.charCodeAt(0),unit,'SOURCE collapsed returned id unit');
  completed++;
}
const input=row(7),returned=exportsObject.collapseHyperliquidOrderHistory([input]);
assert.equal(returned[0],input);input.status='filled';assert.equal(returned[0].status,'filled','SOURCE returned historical winner follows input mutation');completed++;
assert.equal(completed,3,'SOURCE two returned-unit cases and one alias case');
process.stdout.write(JSON.stringify({cases:3,units:['D800','DFFF'],sameWinner:true,last:'live-alias'})+'\n');
