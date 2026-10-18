const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, load} = require('./types-oracle.cjs');
function tree(relative) {
  const file = path.join(root, relative);
  return ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
}
const utils = tree('packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const names = ['toNumber','normalizeMarket'];
const declarations = utils.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => names.includes(d.name.getText(utils))));
assert.equal(declarations.length,2);
const exchange = tree('packages/safe-cex/src/exchanges/weex/weex.exchange.ts');
const cls = exchange.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'WeexExchange');
const field = cls.members.find(n => ts.isPropertyDeclaration(n) && n.name.getText(exchange) === 'fetchMarkets');
assert.ok(field);
const scope = {exports:{}, ENDPOINTS:{EXCHANGE_INFO:'fixture'}, ...load(path.join(root,'packages/ccxt-compat/src/functions/ccxt_exchange.ts'))};
const code = declarations.map(n=>n.getText(utils)).join('\n')+'\nexport class Slice {'+field.getText(exchange)+'}';
vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,scope);
const row = (symbol,baseAsset) => ({symbol,baseAsset,quoteAsset:'USDT',marginAsset:'USDT',pricePrecision:1,quantityPrecision:3,minOrderSize:'0.001',maxOrderSize:'50',minLeverage:'1',maxLeverage:'80',contractVal:'1'});
(async()=>{
  let checks=0;
  for(const field of ['baseAsset','symbol']) for(const value of [undefined,null,17,false,{},[]]) {
    const bad = row('BADUSDT','BAD');
    if(value===undefined) delete bad[field]; else bad[field]=value;
    const rows=[row('ETHUSDT','ETH'),bad,row('SOLUSDT','SOL')];
    const instance = new scope.exports.Slice();
    instance.xhr={get:async()=>({data:{symbols:rows}})};
    instance.store={markets:[{symbol:'CACHED'}]};
    instance.emitAndLogError=()=>assert.fail('source must retain malformed typed fields');
    const actual=await instance.fetchMarkets();
    assert.equal(actual.length,3);
    assert.equal(actual[0].symbol,'ETHUSDT');assert.equal(actual[2].symbol,'SOLUSDT');
    assert.equal(actual[1][field==='baseAsset'?'base':'symbol'],value);
    assert.equal(actual[1].info,bad);
    checks++;
  }
  const instance=new scope.exports.Slice();
  instance.xhr={get:async()=>({data:{symbols:[{...row('BADUSDT',17),makerFeeRate:{toString:null}}]}})};
  instance.store={markets:[{symbol:'CACHED'}]};
  const errors=[];instance.emitAndLogError=message=>errors.push(message);
  assert.equal(await instance.fetchMarkets(),instance.store.markets);
  assert.equal(errors.length,1);assert.equal(errors[0],'Cannot convert object to primitive value');
  checks++;
  assert.equal(checks,13);
  process.stdout.write(String(checks));
})().catch(e=>{console.error(e);process.exit(1);});
