import fs from 'node:fs';import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const runtimeRequire=process.env.SAFE_CEX_TYPESCRIPT_PATH?name=>require(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'..',name)):require;
const root = new URL('../../../', import.meta.url);
const read = path => ts.createSourceFile(path, fs.readFileSync(new URL(path, root), 'utf8'), ts.ScriptTarget.Latest, true);
const compile = source => ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
const variable = (ast, name) => {
  const found = ast.statements.find(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(d => d.name.getText(ast) === name));
  assert.ok(found, `actual source variable ${name}`);
  return found.getText(ast);
};
const publicAst = read('packages/safe-cex/src/exchanges/binance/binance.ws-public.ts');
const parser = read('packages/json-parse/src/jsonParse.ts');
const parserCode = parser.statements.filter(ts.isVariableStatement).map(n=>n.getText(parser)).join('\n');
const control=process.env.SAFE_CEX_CLASS_SOURCE_CONTROL;
const context = {exports:{},require:runtimeRequire};
vm.runInNewContext(compile(`${parserCode}\n${control==='dispatch'?variable(publicAst, 'dispatchBinanceMessage').replace('const json = jsonParse(data);','const json = null;'):variable(publicAst, 'dispatchBinanceMessage')}`), context);
let callbacks = 0, unit;
context.exports.dispatchBinanceMessage({kline:[events=>{callbacks++;unit=events[0].x.charCodeAt(0)}]}, '{"e":"kline","s":"BTC","x":"\\ud800"}', ()=>{});
assert.equal(callbacks, 1, 'actual source escaped surrogate invokes dispatch callback once');
assert.equal(unit, 55296, 'actual source escaped surrogate remains program-observable');

const storeAst = read('packages/safe-cex/src/store/store.base.ts');
const storeClass = storeAst.statements.find(ts.isClassDeclaration);
const update = storeClass.members.find(n=>n.name?.getText(storeAst)==='updateTicker');
const tickerClass = publicAst.statements.find(n=>ts.isClassDeclaration(n) && n.name.text==='BinanceMarketWebsocket');
const tickerMethod = tickerClass.members.find(n=>n.name?.getText(publicAst)==='handleTickerStreamEvents');
const key = {exports:{}, require:runtimeRequire};
vm.runInNewContext(compile(read('packages/safe-cex/src/utils/get-key.ts').text), key);
const alias = {exports:{},v:key.exports.v};
vm.runInNewContext(compile(`class Store {${control==='alias'?update.getText(storeAst).replace('this.tickersBySymbolMap[item.symbol] = item;',''):update.getText(storeAst)}} class Subject {${tickerMethod.getText(publicAst)}} exports.Store=Store;exports.Subject=Subject;`), alias);
const row={id:'btc',symbol:'BTC',last:0};
const store=new alias.exports.Store();
const displaced={id:'eth',symbol:'ETH',last:0};
store.tickersMap={btc:row,eth:displaced};store.tickersBySymbolMap={BTC:row,ETH:displaced};
store.stripTickerInfoFromChanges=value=>value;
let renamed=false;
store.notify=()=>{if(!renamed){renamed=true;store.updateTicker(row,{symbol:'ETH'})}};
Object.defineProperty(store,'tickersBySymbol',{get:()=>store.tickersBySymbolMap});
const subject=new alias.exports.Subject();subject.parent={store};Object.defineProperty(subject,'store',{get:()=>store});
subject.handleTickerStreamEvents([{s:'BTC',c:'4.2',P:'1.2',v:'3.6',q:'9.1'},{s:'ETH',c:'12.3',P:'2.1',v:'5.6',q:'8.1'}]);
assert.equal(row.symbol, 'ETH', 'actual source subscriber renames ticker symbol');
assert.equal(row.last, 12.3, 'actual source same-generation alias receives second price 12.3 rather than 4.2');
assert.equal(store.tickersBySymbolMap.BTC, store.tickersBySymbolMap.ETH, 'actual source preserves both symbol aliases');
assert.equal(store.tickersBySymbolMap.ETH.id,'btc','actual source symbol collision retains latest write ownership');
assert.equal(displaced.last,0,'actual source collision leaves displaced ticker unchanged');

const privateResults=[];
for (const stem of ['binance','binancepm','binancespot']) {
  const ast=read(`packages/safe-cex/src/exchanges/binance/${stem}.ws-private.ts`);
  const klass=ast.statements.find(ts.isClassDeclaration);
  const constants=ast.statements.filter(ts.isVariableStatement).map(n=>n.getText(ast)).join('\n');
  const ctx={exports:{},require:runtimeRequire,v:key.exports.v,clearTimeout(){},setTimeout(){return 42}};
  const types=read(`packages/safe-cex/src/exchanges/binance/${stem}.types.ts`);
  const urls=variable(types,'BASE_WS_URL')+(stem==='binancespot'?'':variable(types,'ENDPOINTS'));
  const base=`class BaseWebSocket {constructor(parent){this.parent=parent;this.isDisposed=false;this.isForceClosed=false}trackLastReceived(){}emitWsDiagnosticReceive(){}recordMessageHandlerStats(){} }`;
  vm.runInNewContext(compile(`${base}\n${urls}\n${parserCode}\n${constants}\n${control==='private-open'?klass.getText(ast).replace('this.ping();','try { this.ping(); } catch {}'):klass.getText(ast)}`),ctx);
  const owner=new ctx.exports[klass.name.text]({options:{}});
  let openThrows=null;
  if(stem!=='binancespot') {
    const marker=new Error('open marker');owner.ping=()=>{throw marker};let caught;
    try{owner.onOpen()}catch(error){caught=error}
    assert.equal(caught,marker,'actual synchronous onOpen preserves original thrown failure');
    assert.equal(owner.hasOpened,false,'actual failed ping leaves hasOpened false');
    openThrows=true;
  }
  let count=0, code;
  owner.handleOrderEvents=events=>{count++;code=events[0].x.charCodeAt(0)};
  const event=stem==='binancespot'?'executionReport':'ORDER_TRADE_UPDATE';
  owner.onMessage({data:`{"e":"${event}","x":"\\ud800"}`});
  assert.equal(count,1,'actual private escaped surrogate invokes order callback once');
  assert.equal(code,55296,'actual private callback observes surrogate code unit');
  if(stem==='binance') {
    const unhandled=[];
    const onRejected=(error,promise)=>unhandled.push({error,promise});
    process.on('unhandledRejection',onRejected);
    try {
      const parent={options:{key:'key',secret:'secret'},xhr:{async post(){return {data:{listenKey:{toString:7,valueOf:8}}}}}};
      const dropped=new ctx.exports[klass.name.text](parent);
      const retained=dropped.connectAndSubscribe();
      await new Promise(resolve=>setImmediate(resolve));
      assert.equal(unhandled.length,1,'actual bare connect reports one unhandled rejection');
      assert.equal(unhandled[0].error.name,'TypeError','actual bare connect rejects uncoercible raw key');
      assert.equal(unhandled[0].promise,retained,'actual reported rejected promise remains retained');
      assert.equal(dropped.connecting,false,'actual bare failed connect clears connecting finally');
    } finally {process.removeListener('unhandledRejection',onRejected)}
  }
  privateResults.push({stem,callbacks:count,unit:code,openThrows});
}
assert.equal(privateResults.length,3,'independent private surrogate floor stops at case2');
process.stdout.write(JSON.stringify({dispatch:{callbacks,unit},aliasLast:row.last,private:privateResults}));
