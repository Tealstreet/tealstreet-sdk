// Execute selected live methods without instantiating network-owning constructors.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
import {loadSource} from './source-loader.mjs';
const base = new URL('../../../packages/safe-cex/src/exchanges/okx/', import.meta.url);
const {ENDPOINTS, BASE_WS_URLS} = loadSource(new URL('okx.types.ts', base));
const {OrderSide, OrderType, PositionMode} = loadSource(new URL('../../types.ts', base));
function selected(file, ownerName, names) {
 const url = new URL(file, base);
 const ast = ts.createSourceFile(url.pathname, fs.readFileSync(url, 'utf8'), ts.ScriptTarget.Latest, true);
 const owner = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === ownerName);
 assert.ok(owner, 'live owner ' + ownerName);
 const members = names.map(name => {
  const member = owner.members.find(n => n.name?.getText(ast) === name);
  assert.ok(member, 'live member ' + name);
  return member.getText(ast);
 });
 const code = ts.transpileModule('export class Selected {' + members.join('\n') + '}', {
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
 }).outputText;
 const module = {exports:{}};
 vm.runInThisContext('(function(module,exports,ENDPOINTS,BASE_WS_URLS,OrderSide,OrderType,PositionMode){' + code + '\n})')(
  module, module.exports, ENDPOINTS, BASE_WS_URLS, OrderSide, OrderType, PositionMode);
 return new module.exports.Selected();
}
const standard = selected('okx.exchange.ts', 'OKXExchange', ['getMarketInstrumentsEndpoint', 'getPositionSide']);
const eu = selected('okxeu.exchange.ts', 'OKXEUExchange', ['getInstrumentTypes', 'getMarketInstrumentsEndpoint']);
const business = selected('okx.ws-business.ts', 'OKXBusinessWebsocket', ['resyncsPrivateStateOnReopen','endpoint']);
const privateSocket = selected('okx.ws-private.ts', 'OKXPrivateWebsocket', ['endpoint']);
const businessAst = ts.createSourceFile('okx.ws-business.ts', fs.readFileSync(new URL('okx.ws-business.ts', base), 'utf8'), ts.ScriptTarget.Latest, true);
const stats = businessAst.statements.filter(ts.isVariableStatement)
 .flatMap(n => [...n.declarationList.declarations]).find(n => n.name.getText(businessAst) === 'ORDER_STATS_TOPIC');
assert.ok(stats?.initializer, 'live order stats constant');
const statsModule = {exports:{}};
vm.runInThisContext('(function(module){' + ts.transpileModule('module.exports=' + stats.initializer.getText(businessAst), {
 compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText + '\n})')(statsModule);
const endpoints = [];
for (const publicDataOnly of [false, true]) {
 standard.isPublicDataOnly = publicDataOnly;eu.isPublicDataOnly = publicDataOnly;
 endpoints.push({publicDataOnly,standard:standard.getMarketInstrumentsEndpoint(),eu:eu.getMarketInstrumentsEndpoint()});
}
const sides = [];
for (const mode of [null, PositionMode.OneWay, PositionMode.Hedged]) {
 standard._accountPositionMode = mode;
 for (const type of [OrderType.Market,OrderType.Limit,OrderType.Conditional,OrderType.StopLoss,OrderType.TakeProfit,OrderType.TrailingStopLoss]) {
  for (const side of [OrderSide.Buy,OrderSide.Sell]) {
   for (const reduceOnly of [undefined,false,true]) {
    const input = {mode,type,side,...(reduceOnly === undefined ? {} : {reduceOnly})};
    try {sides.push({input,ok:standard.getPositionSide({type,side,reduceOnly}) ?? null});}
    catch (error) {sides.push({input,error:{message:error.message,name:error.name}});}
   }
  }
 }
}
assert.equal(sides.length, 108, 'independent closed enum policy corpus');
const sockets = [];
for (const region of ['okx','okxeu']) for (const testnet of [undefined,false,true]) {
 const options = testnet === undefined ? {} : {testnet};
 business.parent = {region,options};privateSocket.parent = {region,options};
 sockets.push({region,...options,private:privateSocket.endpoint,business:business.endpoint});
}
assert.equal(sockets.length, 6, 'independent socket endpoint profiles');
console.log(JSON.stringify({endpoints,instruments:eu.getInstrumentTypes(),resync:business.resyncsPrivateStateOnReopen,
 stats:statsModule.exports,sides,sockets}));
