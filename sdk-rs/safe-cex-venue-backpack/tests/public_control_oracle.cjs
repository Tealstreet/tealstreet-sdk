const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function unused(name){return new Proxy(function(){throw Error('unused '+name);},{get(_t,key){if(key==='__esModule')return false;throw Error('unused '+name+'.'+String(key));}});}
function run(file,require,extra={}){const exports={};const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{fileName:file,reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});assert.equal(compiled.diagnostics.length,0);vm.runInNewContext(compiled.outputText,{exports,require,...extra},{filename:file});return exports;}
const types=run(path.join(root,'packages/safe-cex/src/types.ts'),id=>unused(id));
const constants=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.types.ts'),id=>unused(id));
class Base {constructor(parent){this.parent=parent;this.isDisposed=false;}}
const performance={now(){throw Error('unconfigured time');}};
const {BackpackPublicWebSocket:Socket}=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.ws-public.ts'),id=>{
 if(id==='lodash/throttle')return require(id);
 if(id==='../../types')return types;
 if(id==='./backpack.types')return constants;
 if(id==='../base.ws')return {BaseWebSocket:Base};
 return unused(id);
},{performance});
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const set=s=>Array.from(s,units);
const packet=wire=>{const p=JSON.parse(wire);return {method:p.method,params:p.params===undefined?null:p.params.map(v=>v===null?null:units(v))};};
const symbols=['BTC_USDC','','λ🚀','"\\\u0000\b\t\n\f\r\u001f\u2028\u2029','X\ud800','\udc00Y'];
const intervals=['1m','','λ🚀','"\\\n','\ud800','\udc00'];
const methods=['subscribeToOrderBook','subscribeToTicker','subscribeToTrades','subscribeToBookTicker','subscribeToKline'];
const prefixes=['depth.','ticker.','trade.','bookTicker.','kline.'];
const subscribe=[];
for(let m=0;m<5;m++)for(let i=0;i<6;i++)for(const existing of [false,true])for(const fail of [false,true]){
 const s=new Socket({}),symbol=symbols[i],interval=intervals[i];
 const stream=m===4?`kline.${interval}.${symbol}`:prefixes[m]+symbol;
 s.subscriptions=new Set(['keep',...(existing?[stream]:[])]);
 const failure={},packets=[],wires=[],before=[];
 s.send=wire=>{before.push(set(s.subscriptions));packets.push(packet(wire));wires.push(units(wire));s.subscriptions.delete('keep');s.subscriptions.add('after');if(fail)throw failure;};
 let sameError=false;try{s[methods[m]](symbol,interval);}catch(e){assert.equal(e,failure);sameError=true;}
 subscribe.push({method:methods[m],symbol:units(symbol),interval:units(interval),existing,fail,packets,wires,before,after:set(s.subscriptions),sameError});
}
const seeds=[[],['a'],['b','a','b'],['\ud800','\udc00','🚀'],['a','b','a'],['','"\\\n']];
const resubscribe=[];
for(let i=0;i<6;i++)for(const fail of [false,true]){
 const s=new Socket({});s.subscriptions=new Set(seeds[i]);if(i===4){s.subscriptions.delete('a');s.subscriptions.add('a');}
 const failure={},packets=[],wires=[];s.send=wire=>{packets.push(packet(wire));wires.push(units(wire));s.subscriptions.add('after');if(fail)throw failure;};
 let sameError=false;try{s.resubscribe();}catch(e){assert.equal(e,failure);sameError=true;}
 resubscribe.push({seed:seeds[i].map(units),reinsert:i===4,fail,packets,wires,after:set(s.subscriptions),sameError});
}
const unsubscribe=[];
const streamCases=[[],['a'],['a','b','a'],['absent'],['\ud800',null,'c'],['a'],['a'],['a']];
for(let i=0;i<8;i++)for(const fail of [false,true]){
 const s=new Socket({});s.subscriptions=new Set(['a','b','c','keep','\ud800']);
 const streams=streamCases[i].slice();if(i===4)delete streams[1];
 const failure={},packets=[],wires=[],before=[];
 s.send=wire=>{before.push(set(s.subscriptions));packets.push(packet(wire));wires.push(units(wire));if(i===5)streams.push('b');if(i===6)streams.length=0;if(i===7)streams[0]='c';if(fail)throw failure;};
 let sameError=false;try{s.unsubscribe(streams);}catch(e){assert.equal(e,failure);sameError=true;}
 unsubscribe.push({mode:i,streams:streamCases[i].map(v=>v===null?null:units(v)),fail,packets,wires,before,after:set(s.subscriptions),sameError});
}
const kline=[];
for(let i=0;i<6;i++)for(const fail of [false,true]){
 const s=new Socket({}),failure={},forwarded=[];s.unsubscribe=streams=>{forwarded.push(streams.map(units));if(fail)throw failure;};
 let sameError=false;try{s.unsubscribeFromKline(symbols[i],intervals[i]);}catch(e){assert.equal(e,failure);sameError=true;}
 kline.push({symbol:units(symbols[i]),interval:units(intervals[i]),fail,forwarded,sameError});
}
const bits=n=>{if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const ping=[];
const times=[0,-0,NaN,Infinity,-Infinity,123.25];
const pingCases=[...times.map(now=>({disposed:false,connected:true,failure:'none',now})),
 ...[true,false].flatMap(disposed=>[true,false].flatMap(connected=>['none','connected'].map(failure=>({disposed,connected,failure,now:7})))),
 ...['now','send'].flatMap(failure=>[true,false].map(connected=>({disposed:false,connected,failure,now:7})))];
for(const c of pingCases){
 const s=new Socket({}),failure={},calls=[],packets=[],wires=[];s.isDisposed=c.disposed;s.pingAt=99;
 Object.defineProperty(s,'isConnected',{get(){calls.push('connected');if(c.failure==='connected')throw failure;return c.connected;}});
 performance.now=()=>{calls.push('now');if(c.failure==='now')throw failure;return c.now;};
 s.send=wire=>{calls.push('send');packets.push(packet(wire));wires.push(units(wire));if(c.failure==='send')throw failure;};
 let sameError=false;try{s.ping();}catch(e){assert.equal(e,failure);sameError=true;}
 ping.push({...c,now:bits(c.now),pingAt:bits(s.pingAt),calls,packets,wires,sameError});
}
const open=[];
for(const disposed of [false,true])for(const fail of ['none','track','reset','resubscribe','ping']){
 const s=new Socket({}),failure={},calls=[];s.isDisposed=disposed;
 for(const [method,label] of [['trackLastReceived','track'],['resetAndRefetchOrderBooks','reset'],['resubscribe','resubscribe'],['ping','ping']])s[method]=()=>{calls.push(label);s.isDisposed=true;if(fail===label)throw failure;};
 let sameError=false;try{s.onOpen();}catch(e){assert.equal(e,failure);sameError=true;}
 open.push({disposed,fail,calls,sameError});
}
process.stdout.write(JSON.stringify({subscribe,resubscribe,unsubscribe,kline,ping,open}));
