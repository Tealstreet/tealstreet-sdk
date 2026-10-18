// Maintained constructors, topic bodies and parent selectors; host sockets/time are scripted.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const {TinyEmitter} = require('tiny-emitter');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const read = f => fs.readFileSync(path.join(root,'packages/safe-cex/src',f),'utf8');
const compilerOptions = {target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS};
const context = vm.createContext({
  chunk:require('lodash/chunk'), flatten:require('lodash/flatten'),
  ENABLE_RECONNECTING_WEBSOCKET:false, sleep:async()=>{},
});
function evaluate(s) { vm.runInContext(ts.transpileModule(s,{compilerOptions}).outputText,context); }
function selected(file,className,names) {
  const a=ts.createSourceFile(file,read(file),ts.ScriptTarget.Latest,true);
  const cls=a.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===className);assert(cls,className);
  return names.map(name=>{
    const rows=cls.members.filter(n=> name==='constructor' ? ts.isConstructorDeclaration(n) : n.name?.getText(a)===name);
    assert.equal(rows.length,1,`${file}:${name}`);return rows[0].getText(a);
  }).join('\n');
}
const base = ts.createSourceFile('base.ts',read('exchanges/base.ts'),ts.ScriptTarget.Latest,true);
const normalize=base.statements.filter(ts.isVariableStatement).find(n=>n.declarationList.declarations.some(d=>d.name.getText(base)==='normalizeSymbols'));
assert(normalize,'actual normalizeSymbols');evaluate(normalize.getText(base)+';globalThis.normalize=normalizeSymbols;');
evaluate(`globalThis.Parent = class {
 ${selected('exchanges/base.ts','BaseExchange',['getActiveTickerSymbols','setActiveTickerSymbols','getTickerSymbolsForSubscription'])}
 ${selected('exchanges/bybit/bybitspot.exchange.ts','BybitSpotExchange',['getTickerSymbolsForPublicStream'])}
};`);
class SocketHost {
  constructor(parent) {this.parent=parent; this.store=parent.store; this.frames=[];this.connected=false;this.ws=undefined;}
  get isDisposed(){return this.parent.isDisposed;}
  get isConnected(){return this.connected;}
  send(s){this.frames.push(JSON.parse(s));}
  ping(){this.send('{"op":"ping"}');}
  armExistingOhlcvNoDataTimeouts(){}
  emitWsReconnected(){throw Error('unexpected reconnecting mode');}
  dispose(){this.ws=undefined;}
}
context.BaseWebSocket=SocketHost;
evaluate(`globalThis.Primary=class extends BaseWebSocket {
 ${selected('exchanges/bybit/bybit.ws-public.ts','BybitPublicWebsocket',['topics','constructor','onOpen','onMarketsChanged','subscribe'])}
};`);
evaluate(`globalThis.Spot=class extends BaseWebSocket {
 ${selected('exchanges/bybit/bybitspot.ws-public.ts','BybitPublicWebsocket',['topics','handleActiveTickerSymbolsChanged','constructor','getCurrentTickerTopics','getDesiredTickerTopics','syncTickerTopics','onOpen','onMarketsChanged','subscribe','dispose'])}
};`);
const flush = async()=>{for(let i=0;i<32;i++)await Promise.resolve();};
(async()=>{
  const inputs=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(inputs.length,24,'fixed source scenario count');
  const out=[];
  for(const input of inputs){
    const parent=new context.Parent();parent.emitter=new TinyEmitter();parent.on=parent.emitter.on.bind(parent.emitter);parent.off=parent.emitter.off.bind(parent.emitter);parent.store={markets:input.markets};parent._activeTickerSymbols=context.normalize(input.active);parent.isDisposed=false;
    const subject=input.category==='spot'?new context.Spot(parent):new context.Primary(parent,input.category);
    const snapshots=[];
    for(const action of input.actions){
      if(action.op==='active')parent.setActiveTickerSymbols(action.symbols);
      else if(action.op==='open'){subject.ws={};subject.connected=true;subject.onOpen();}
      else if(action.op==='offline')subject.connected=false;
      else if(action.op==='markets')parent.emitter.emit('markets:changed',{added:action.added,removed:[]});
      else if(action.op==='dispose'){subject.dispose();}
      else if(action.op==='parentDispose')parent.isDisposed=true;
      else throw Error(action.op);
      await flush();snapshots.push([...subject.frames]);
    }
    out.push(snapshots);
  }
  assert.equal(out.length,24);process.stdout.write(JSON.stringify(out));
})().catch(e=>{console.error(e);process.exitCode=1;});
