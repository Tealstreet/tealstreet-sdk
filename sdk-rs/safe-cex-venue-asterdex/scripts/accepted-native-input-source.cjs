const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');
const sourceFile = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts');
const ast = ts.createSourceFile(sourceFile, fs.readFileSync(sourceFile, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const names = ['handleOrderTradeUpdate', 'mirrorCumulativeOrderFill', 'getOrderUpdateTime', 'scheduleOrderReconcile', 'clearOrderReconcileTimer', 'ORDER_RECONCILE_DEBOUNCE_MS'];
const printer = ts.createPrinter();
const members = names.map(name => {
  const member = cls.members.find(node => node.name?.getText(ast) === name);
  assert.ok(member, name);
  return printer.printNode(ts.EmitHint.Unspecified, member, ast);
});
const code = ts.transpileModule(`return class AsterDEXPrivateWebsocket { ${members.join('\n')} }`, {
  compilerOptions: { target: ts.ScriptTarget.ES2022 },
}).outputText;
const timers = [];
const Private = new Function('LogSeverity', 'OrderStatus', 'OrderSide', 'virtualClock', 'setTimeout', 'clearTimeout', code)(
  {Debug:'debug'}, {Closed:'closed',Open:'open'}, {Buy:'buy',Sell:'sell'}, {getCurrentTimeMs:()=>1000},
  (callback, delay) => { timers.push({callback,delay}); return timers.length; }, () => {},
);
const units = value => Array.from({length:value.length}, (_, i) => value.charCodeAt(i));
const tag = value => {
  if (value === undefined) return {kind:'undefined'};
  if (value === null) return {kind:'null'};
  if (typeof value === 'number') {
    const buffer = Buffer.alloc(8); buffer.writeDoubleBE(value);
    return {kind:'number',bits:buffer.toString('hex')};
  }
  if (typeof value === 'string') return {kind:'string',units:units(value)};
  if (typeof value === 'boolean') return {kind:'boolean',value};
  return {kind:'json',units:units(JSON.stringify(value))};
};
const rawSymbols = ['21','false','null','{"nested":[1,"\\ud800"]}','["BTC",2]','"BTCUSDT"','"\\ud800"','-0','1e400',undefined];
const fillCases = rawSymbols.map((symbol, index) => {
  const frame = `{"e":"ORDER_TRADE_UPDATE","o":{"X":"FILLED","i":${77+index},${symbol === undefined ? '' : `"s":${symbol},`}"S":"BUY","l":"1","L":"2","q":"1","z":"1","T":1000}}`;
  const socket = new Private();
  const fills = [];
  socket.parent = {store:{orders:[],updateOrder(){}},log(){},emitter:{emit(name,value){if(name==='fill')fills.push(value);}}};
  socket.handleOrderTradeUpdate(JSON.parse(frame));
  assert.equal(fills.length,1);
  assert.equal(Object.hasOwn(fills[0],'symbol'),true);
  assert.deepEqual(tag(fills[0].symbol),tag(JSON.parse(frame).o.s));
  return {frame,symbol:tag(fills[0].symbol)};
});
assert.equal(timers.length,fillCases.length);
assert(timers.every(timer => timer.delay === 10000));
const {createRequire} = require('node:module');
const ethers = createRequire(path.join(root,'packages/safe-cex/package.json'))('ethers');
const crypto = require('node:crypto');
const apiPath = path.join(root,'packages/safe-cex/src/exchanges/asterdex/asterdex.api.ts');
const api = ts.createSourceFile(apiPath,fs.readFileSync(apiPath,'utf8'),ts.ScriptTarget.Latest,true);
const declaration = api.statements.flatMap(n => ts.isVariableStatement(n) ? [...n.declarationList.declarations] : []).find(n => n.name.getText(api)==='generateV1Signature');
assert(declaration);
const signatureCode = ts.transpileModule(`return (${declaration.initializer.getText(api)});`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const sign = new Function('crypto',signatureCode)(crypto);
const cryptoCases = [[],[0,127,255],[1,2,128]].map(bytes => ({
  bytes,hex:ethers.hexlify(Uint8Array.from(bytes)),
  keccak:[...ethers.getBytes(ethers.keccak256(Uint8Array.from(bytes)))],
  hashMessage:[...ethers.getBytes(ethers.hashMessage(Uint8Array.from(bytes)))],
}));
const signatures = ['symbol=BTCUSDT&timestamp=1000','é🦀'].map(query => ({query,secret:'fixture-secret',signature:sign(query,'fixture-secret')}));
console.log(JSON.stringify({fillCases,timerDelay:10000,cryptoCases,signatures}));
