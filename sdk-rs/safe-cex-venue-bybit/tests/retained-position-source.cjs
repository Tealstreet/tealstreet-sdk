const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const declaration = source.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'BybitExchange');
const names = ['getPositionUpdatedTime', 'getPositionReliabilityScore', 'dedupePositionsById'];
const methods = names.map(name => {
  const found = declaration.members.filter(node => node.name?.getText(source) === name);
  assert.equal(found.length, 1, name);
  return found[0].getText(source);
});
const context = vm.createContext({});
vm.runInContext(ts.transpileModule(`globalThis.Subject=class {${methods.join('\n')}};`, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText, context);
const row = {id:'p',contracts:2,info:{updatedTime:'1700000000000',seq:7,positionStatus:'Normal',avgPrice:' '}};
const selected = new context.Subject().dedupePositionsById([row,{...row,info:{...row.info,avgPrice:'\ud800'}}]);
assert.equal(selected.length,1);
assert.equal(selected[0].info.avgPrice,'\ud800');
const nativeMarkets = [];
const coercionErrors = [];
for (const variant of ['bybit','bybitinverse','bybitspot']) {
  const filename = path.resolve(path.dirname(file), `${variant}.exchange.ts`);
  const syntax = ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
  const cls = syntax.statements.find(node => ts.isClassDeclaration(node) && node.members.some(member => member.name?.getText(syntax)==='fetchMarketsHelper'));
  const method = cls.members.find(member => member.name?.getText(syntax)==='fetchMarketsHelper');
  let mapper;
  function walk(node) {
    if(ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression) && node.expression.name.text==='map' && ts.isArrowFunction(node.arguments[0])) mapper=node.arguments[0];
    ts.forEachChild(node,walk);
  }
  walk(method); assert.ok(mapper);
  const scope=vm.createContext({fees:[],category:variant==='bybitspot'?'spot':variant==='bybitinverse'?'inverse':'linear'});
  vm.runInContext(ts.transpileModule(`globalThis.map=${mapper.getText(syntax)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,scope);
  const raw={symbol:'BTCUSDT',baseCoin:'BTC',quoteCoin:'USDT',status:'Trading',lotSizeFilter:{qtyStep:'0.001',minOrderQty:'0.001',maxOrderQty:'100'},priceFilter:{tickSize:'0.1'},leverageFilter:{minLeverage:'1',maxLeverage:'100'},deliveryTime:'0',missing:undefined,nan:NaN,zero:-0,['\ud801']:'\ud800'};
  const market=scope.map(raw);
  assert.equal(market.info,raw); assert.ok(Object.hasOwn(market.info,'missing'));
  assert.equal(market.info.missing,undefined); assert.ok(Number.isNaN(market.info.nan)); assert.ok(Object.is(market.info.zero,-0));
  assert.equal(market.info['\ud801'].charCodeAt(0),0xd800);
  try { scope.map({...raw,lotSizeFilter:{...raw.lotSizeFilter,qtyStep:{toString:null}}}); }
  catch (error) { coercionErrors.push({variant,name:error.name,message:error.message}); }
  assert.throws(()=>scope.map({...raw,lotSizeFilter:undefined}),/Cannot read properties of undefined/);
  if(variant!=='bybitspot') {
    scope.fees=[{takerFeeRate:{toString:null},makerFeeRate:'0'}];
    assert.throws(()=>scope.map({...raw,symbol:undefined}),/Cannot convert object to primitive value/);
  }
  nativeMarkets.push(variant);
}
const optionExports={};
const optionsFile=path.resolve(path.dirname(file),'bybit.options.ts');
vm.runInNewContext(ts.transpileModule(fs.readFileSync(optionsFile,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,{exports:optionExports});
assert.equal(optionExports.mapBybitOptionInstrument({status:'Trading',symbol:'SOL\ud800-6NOV26-0-P',optionsType:'Put'}),undefined);
assert.equal(optionExports.mapBybitOptionInstrument({status:'Trading',symbol:'SOL\ud800-6NOV26-112d5-P',optionsType:'Unknown'}),undefined);
assert.equal(coercionErrors.length,3);
process.stdout.write(JSON.stringify({units:[selected[0].info.avgPrice.charCodeAt(0)],nativeMarkets,coercionErrors}));
