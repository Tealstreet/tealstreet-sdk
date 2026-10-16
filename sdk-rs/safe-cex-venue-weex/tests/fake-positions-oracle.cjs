const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const {root,ts,header,capture,load}=require('./types-oracle.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const selected=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)==='fakePositionsForMarket'));
assert.equal(selected.length,1,'fake_position_source_inventory');
const enums=load(path.join(root,'packages/safe-cex/src/types.ts'));
const context={exports:{},...enums,...load(path.join(root,'packages/safe-cex/src/utils/createPositionId.ts'))};
vm.runInNewContext(ts.transpileModule(selected[0].getText(ast),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
const cases=[];
for(const symbol of ['BTC-USDT','', 'é𝒜:short'])for(const mode of Object.values(enums.PositionMode))for(const margin of Object.values(enums.MarginType))for(const leverage of [13.75,0,-0,-3.25,NaN,Infinity,-Infinity]){
 const market={id:'distinct-id',symbol,base:'BTC',quote:'USDT',active:true,precision:{amount:.001,price:.01},limits:{amount:{min:.1,max:123},leverage:{min:1,max:99}},info:{mustNotCopy:7},makerFee:.002,takerFee:.003};
 const native=context.exports.fakePositionsForMarket(market,mode,margin,leverage);
 cases.push({market,mode,margin,leverage:capture(leverage),positions:native.map(p=>Object.fromEntries(Object.entries(p).map(([k,v])=>[k,k==='info'?{kind:Array.isArray(v)?'array':typeof v,keys:Object.keys(v)}:capture(v)])))});
}
const refusal=context.exports.fakePositionsForMarket({symbol:'\ud800'},enums.PositionMode.Hedged,enums.MarginType.Cross,13);
assert.deepEqual(capture(refusal[0].symbol),{utf16:[0xd800]},'fake_position_source_utf16');
process.stdout.write(JSON.stringify({header,cases,refusal:'{"id":"i","symbol":"\\ud800","base":"B","quote":"Q","active":true,"precision":{"amount":1,"price":2},"limits":{"amount":{"min":3,"max":4},"leverage":{"min":5,"max":6}}}'}));
