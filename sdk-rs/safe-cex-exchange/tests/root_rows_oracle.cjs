const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {root,ts}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const mode=process.argv[2];
function fields(file,names){const text=fs.readFileSync(path.join(root,file),'utf8');const ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BaseWebSocket'||ts.isClassDeclaration(n)&&n.name.text==='BaseExchange');const members=cls.members.filter(n=>names.includes(n.name?.getText(ast)));assert.equal(members.length,names.length,'ROOT source independent member count');return members.map(n=>n.getText(ast)).join('\n');}
class Rws {addEventListener(){};static OPEN=1;readyState=0;calls=0;reconnect(){this.calls++}}
const code=ts.transpileModule('class Witness { '+fields('packages/safe-cex/src/exchanges/base.ws.ts',['reconnectNow','retryFailedConnect','recoverAfterOffline','resetFailureStreak','resetReconnectBackoff'])+' }; return Witness;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const cleared=[],W=new Function('lifecycleLog','LogSeverity','ReconnectingWebSocket','WebSocket','clearTimeout','Date',code)(()=>{}, {Warning:'warning'}, Rws,{CONNECTING:0},id=>cleared.push(id),{now:()=>1000000});
function fixture(){const w=new W();w.parent={name:'venue',isDisposed:false,options:{key:'k'},log:(msg,level)=>w.logs.push([msg,level]),emitAndLogError:msg=>w.errors.push(msg)};w.logs=[];w.errors=[];w.closes=[];w.connects=0;w.forceClosed=false;w.reconnecting=true;w.recoversAfterOffline=true;w.connectFailureAnnounced=false;w.connectAndSubscribe=()=>w.connects++;w.onClose=e=>w.closes.push(e.code);w.emitWsReconnected=()=>{};return w;}
if(mode==='reconnect') {const w=fixture();w.reconnectTimer=19;const listeners=[];w.ws={addEventListener:(...x)=>listeners.push(x)};w.reconnectNow();assert.equal(w.connects,1);assert.equal(w.reconnectTimer,undefined);assert.equal(cleared.length,0);assert.equal(w.reconnecting,false);assert.equal(w.lastCloseTime,1000000);assert.equal(listeners[0][0],'open');console.log(JSON.stringify({connects:1,lastCloseTime:w.lastCloseTime,cancelled:0}));}
else if(mode==='retry'){const w=fixture();w.retryFailedConnect('first');w.retryFailedConnect('second');assert.deepEqual(w.errors,['first']);assert.deepEqual(w.logs,[['second','warning']]);assert.deepEqual(w.closes,[1006,1006]);w.resetFailureStreak();assert.equal(w.connectFailureAnnounced,false);w.retryFailedConnect('third');assert.equal(w.errors.length,2);w.parent.emitAndLogError=()=>{throw Error('reporter failed')};w.resetFailureStreak();assert.throws(()=>w.retryFailedConnect('bad'),/reporter failed/);assert.equal(w.connectFailureAnnounced,false);assert.equal(w.closes.length,3);console.log(JSON.stringify({announcements:2,closes:3}));}
else if(mode==='recovery'){let iterations=0;for(const managed of [true,false])for(const ready of [0,1,2,3]){const w=fixture();const io=managed?new Rws():{readyState:ready,calls:0,closed:0,addEventListener(){},close(){this.closed++;this.args=Array.from(arguments)}};io.readyState=ready;w.ws=io;w.onCloseCount=101;w.reconnectCount=102;assert.equal(w.recoverAfterOffline(),true);assert.equal(io.calls,Number(managed&&ready===1));assert.equal(w.onCloseCount,managed?101:0);if(!managed){assert.equal(io.closed,Number(ready!==0));if(ready!==0)assert.deepEqual(io.args,[],"ROOT source recovery omits close arguments");assert.equal(w.connects,Number(ready===0));}iterations++;}assert.equal(iterations,8);const w=fixture();w.recoversAfterOffline=false;assert.equal(w.recoverAfterOffline(),false);w.parent.isDisposed=true;assert.equal(w.recoverAfterOffline(),true);console.log(JSON.stringify({iterations}));}
else if(mode==='base'){const body=fields('packages/safe-cex/src/exchanges/base.ts',['recoverAfterOffline']);const C=new Function(ts.transpileModule('class Witness { '+body+' }; return Witness;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)();const w=new C(),calls=[];w.getAllSockets=()=>[0,1,2].map(i=>({recoverAfterOffline:()=>{calls.push(i);return [false,undefined,true][i]}}));assert.equal(w.recoverAfterOffline(),false);assert.deepEqual(calls,[0,1,2]);calls.length=0;w.getAllSockets=()=>[0,1,2].map(i=>({recoverAfterOffline:()=>{calls.push(i);if(i===1)throw Error('recover failed');return true}}));assert.throws(()=>w.recoverAfterOffline(),/recover failed/);assert.deepEqual(calls,[0,1]);console.log(JSON.stringify({eager:3,throwPrefix:2}));}
else if (mode === 'fill') {
  const file = 'packages/safe-cex/src/exchanges/fillEventUnits.ts';
  const text = fs.readFileSync(path.join(root, file), 'utf8');
  const ex = {};
  new Function('exports', ts.transpileModule(text, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText)(ex);
  const keys = ['bybit','bybitdemo','bybitinverse','bybitspot','hyrotrader','bitrue',
    'binance','binancepm','binancespot','hyperliquid','hyperliquidspot','grvt',
    'asterdex','weex','backpack','okx','okxeu','okxdemo','okxeudemo','kucoin','blofin','mexc'];
  assert.deepEqual(ex.FILL_EVENT_UNIT_VENUES, keys);
  for (let i = 0; i < 22; i++) {
    assert.equal(ex.fillEventUnits(keys[i]), i < 15 ? 'native' : 'base');
  }
  const typesFile = path.join(root, 'packages/safe-cex/src/types.ts');
  const ast = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'),
    ts.ScriptTarget.Latest, true);
  const names = ast.statements.find(n => ts.isTypeAliasDeclaration(n) && n.name.text === 'ExchangeName');
  assert.ok(names && ts.isUnionTypeNode(names.type), 'ROOT reached ExchangeName union');
  const venues = names.type.types.map(n => {
    assert.ok(ts.isLiteralTypeNode(n) && ts.isStringLiteral(n.literal));
    const name = n.literal.text;
    const unit = ex.fillEventUnits(name);
    assert.ok(unit === undefined || unit === 'native' || unit === 'base',
      'ROOT admitted venue never yields an inherited function');
    return { name, unit: unit ?? null };
  });
  assert.equal(venues.length, 49, 'ROOT maintained ExchangeName count');
  const prototype = Object.getOwnPropertyNames(Object.prototype).map(name => {
    const value = ex.fillEventUnits(name);
    assert.equal(value, name === '__proto__' ? Object.prototype : Object.prototype[name],
      'ROOT inherited builtin identity');
    return { name, length: value.length ?? null, text: String(value) };
  });
  const absent = ['', 'missing', 'TOSTRING', 'constructorX', 'venue🚀'];
  for (const name of absent) assert.equal(ex.fillEventUnits(name), undefined);
  console.log(JSON.stringify({ keys, venues, prototype, absent,
    native: 15, base: 7, prototypeArity: ex.fillEventUnits('toString').length }));
}
else if(mode==='types'){const text=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/types.ts'),'utf8'),ast=ts.createSourceFile('types.ts',text,ts.ScriptTarget.Latest,true);const defs=ast.statements.filter(n=>ts.isTypeAliasDeclaration(n)&&['MessageHandlersStats','MessageHandlers'].includes(n.name.text));assert.equal(defs.length,2);const stat=defs[0].type.members[0].type.members;assert.deepEqual(stat.map(n=>n.name.text),['lastMessageTimestamp','lastCallbacksCount','hadHandlers']);assert.deepEqual(stat.map(n=>!!n.questionToken),[false,true,true]);assert.equal(defs[1].type.members[0].type.elementType.type.parameters[0].dotDotDotToken.kind,ts.SyntaxKind.DotDotDotToken);console.log(JSON.stringify({statFields:3,optionalFields:2,aliases:2}));}
else throw Error('unknown mode');
