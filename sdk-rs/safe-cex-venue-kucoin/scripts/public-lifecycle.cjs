const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),{root}=require('./tables.cjs');
function code(venue,members){
 const file=path.join(root,`packages/safe-cex/src/exchanges/kucoin/${venue}.ws-public.ts`),ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const cls=ast.statements.find(ts.isClassDeclaration),body=members.map(name=>{const m=cls.members.find(n=>n.name?.getText(ast)===name);assert.ok(m,'maintained '+name);return m.getText(ast);});
 const constant=members.includes('onMarketsChanged')?ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)==='SNAPSHOT_CHANNEL')).getText(ast):'';
 return ts.transpileModule(constant+'\nexport class Selected {'+body.join('\n')+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
}
const codes={futures:code('kucoin',['onOpen']),spot:code('kucoinspot',['onOpen']),lookup:code('kucoin',['tickerForApiSymbol']),changes:code('kucoin',['onMarketsChanged'])};
function owner(source,sandbox={}){const module={exports:{}};vm.runInNewContext(source,{...sandbox,module,exports:module.exports});return new module.exports.Selected();}
const text=v=>String.fromCharCode(...v),units=v=>Array.from({length:v.length},(_,i)=>v.charCodeAt(i));
const input=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(input.futures.length,48);assert.equal(input.spot.length,48);assert.equal(input.lookup.length,16);assert.equal(input.changes.length,25);
function opens(cases,source){return cases.map(c=>{
 const events=[],failure=new TypeError('public callback failure');let enabled=c.enabled,disposed=c.disposed,index=0;
 const sandbox={module:{exports:{}}};sandbox.exports=sandbox.module.exports;Object.defineProperty(sandbox,'ENABLE_RECONNECTING_WEBSOCKET',{get(){events.push('enabled');return enabled;}});
 vm.runInNewContext(source,sandbox);const receiver=new sandbox.module.exports.Selected();Object.defineProperty(receiver,'isDisposed',{get:()=>disposed});
 const invoke=action=>{events.push(action);const i=index++;if(i===c.flip){disposed=true;enabled=!enabled;}if(i===c.failAt)throw failure;};
 receiver.subscribe=()=>invoke('subscribe');receiver.ping=()=>invoke('ping');receiver.onMarketsChanged=()=>{};
 receiver.parent={emitter:{off:(event,handler)=>{assert.equal(event,'markets:changed');assert.equal(handler,receiver.onMarketsChanged);invoke('emitter.off:'+event);receiver.onMarketsChanged=()=>{};},on:(event,handler)=>{assert.equal(event,'markets:changed');assert.equal(handler,receiver.onMarketsChanged);invoke('emitter.on:'+event);}}};
 receiver.emitWsReconnected=()=>invoke('emitWsReconnected');let error=null;try{receiver.onOpen();}catch(e){assert.equal(e,failure);error={name:e.name,message:e.message,same:true};}return {events,error};
});}
const futures=opens(input.futures,codes.futures),spot=opens(input.spot,codes.spot),lookup=[];
for(const c of input.lookup){
 const receiver=owner(codes.lookup),failure=new TypeError('public callback failure'),wanted={value:17};let calls=0;
 const table=Object.create(null);Object.defineProperty(table,'converted',{get:()=>c.present?wanted:undefined});
 receiver.parent={store:{get tickersBySymbol(){calls++;if(c.fail)throw failure;return table;}},toInternalSymbol:api=>{assert.deepEqual(units(api),c.symbol);return 'converted';}};
 let outcome;try{const value=receiver.tickerForApiSymbol(c.symbol===null?undefined:text(c.symbol));assert.equal(value,c.symbol?.length&&c.present?wanted:undefined);outcome={value:value?.value??null};}
 catch(e){assert.equal(e,failure);outcome={error:{name:e.name,message:e.message,same:true}};}lookup.push({calls,outcome});
}
function market(tag,id,symbol){return {tag,id:id===null?undefined:text(id),symbol};}
const changes=[];
for(const c of input.changes){
 const scenario=c.scenario,receiver=owner(codes.changes),events=[],sent=[],failure=new TypeError('public callback failure');
 let added=scenario==='empty'?[]:[market(0,[65],'ignored'),market(1,[],'fallback'),market(2,[65],'ignored'),market(3,[0xd800,66],'ignored'),market(4,[79,70,70],'ignored'),market(5,null,'empty')];
 if(scenario!=='empty')added.length=7;
 let disposed=scenario==='disposed';receiver.role=scenario==='detail'?'detail':'snapshot';
 Object.defineProperty(receiver,'ws',{get(){events.push('ws');if(scenario==='ws-error')throw failure;if(scenario==='ws-clear')added.length=0;return scenario==='no-ws'?undefined:{};}});
 Object.defineProperty(receiver,'isDisposed',{get:()=>disposed});Object.defineProperty(receiver,'isConnected',{get(){events.push('connected');if(scenario==='connected-error')throw failure;if(scenario==='connected-dispose')disposed=true;if(scenario==='connected-role')receiver.role='detail';return scenario!=='disconnected';}});
 receiver.snapshotShardSymbols=()=>{events.push('shard');if(scenario==='shard-error')throw failure;if(scenario==='shard-append')added.push(market(7,[65],'ignored'));if(scenario==='shard-clear')added.length=0;if(scenario==='shard-flip'){disposed=true;receiver.role='detail';}return [[65],[66],[0xd800,66],[65]].map(text);};
 const decorate=m=>{
  const id=m.id;Object.defineProperty(m,'id',{get(){
   events.push('market:'+m.tag);if(scenario==='map-error'&&m.tag===0)throw failure;
   if(m.tag===0){if(scenario==='map-append')added.push(decorate(market(7,[65],'ignored')));if(scenario==='map-delete')delete added[1];if(scenario==='map-replace')added[2]=decorate(market(8,[66],'ignored'));}
   return id;
  }});return m;
 };
 for(const m of added)if(m)decorate(m);
 if(scenario==='inherited'){
  const proto=Object.create(Array.prototype);proto[6]=decorate(market(9,[66],'ignored'));Object.setPrototypeOf(added,proto);
 }
 if(scenario==='index-getter'){
  const current=added[1];Object.defineProperty(added,'1',{get(){events.push('index:1');added[2]=decorate(market(8,[66],'ignored'));return current;},configurable:true});
 }
 if(scenario==='index-error')Object.defineProperty(added,'0',{get(){throw failure;},configurable:true});
 if(scenario==='length-error')added=new Proxy(added,{get(target,key,receiver){if(key==='length')throw failure;return Reflect.get(target,key,receiver);}});
 const originalShard=receiver.snapshotShardSymbols;
 receiver.snapshotShardSymbols=()=>{const values=originalShard();if(scenario==='shard-append')decorate(added[7]);return values;};
 receiver.parent={toApiSymbol:symbol=>{const tag=symbol==='fallback'?1:5;events.push('api:'+tag);if(scenario==='api-error'&&tag===1)throw failure;return symbol==='fallback'?'B':'';}};
 receiver.sendChannelOp=(operation,channels,symbols)=>{events.push('send');sent.push({operation,channels,symbols:symbols.map(units)});if(scenario==='send-error')throw failure;};
 let error=null;try{receiver.onMarketsChanged({added});}catch(e){assert.equal(e,failure);error={name:e.name,message:e.message,same:true};}changes.push({events,sent,error});
}
process.stdout.write(JSON.stringify({futures,spot,lookup,changes}));
