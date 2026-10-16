import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';import {ts}from '../../safe-cex-store/tests/source_loader.mjs';
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function ast(path){return ts.createSourceFile(path,fs.readFileSync(path,'utf8'),ts.ScriptTarget.Latest,true);}
function member(file,ownerName,name){const a=ast(file),o=a.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===ownerName),m=o.members.find(n=>n.name?.getText(a)===name);assert.ok(m,'maintained '+ownerName+'.'+name);return m.getText(a);}
function variables(file,names){const a=ast(file);return a.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(a)))).map(n=>n.getText(a)).join('\n');}
const files={private:new URL('exchanges/okx/okx.ws-private.ts',root).pathname,business:new URL('exchanges/okx/okx.ws-business.ts',root).pathname};
const methods={private:member(files.private,'OKXPrivateWebsocket','onMessage'),business:member(files.business,'OKXBusinessWebsocket','onMessage')};
const constants=variables(new URL('exchanges/okx/okx.types.ts',root).pathname,['INTERVAL'])+'\n'+variables(files.business,['TIMEFRAME_BY_CANDLE_CHANNEL','ORDER_CHANNELS','ORDER_STATS_TOPIC']);
const parseFile=new URL('../../../packages/json-parse/src/jsonParse.ts',import.meta.url).pathname;
const parser={exports:{}};vm.runInThisContext('(function(module,exports){'+ts.transpileModule(variables(parseFile,['jsonParse']),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText+'\n})')(parser,parser.exports);
const input=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(input.routes.length,72,'independent message floor');assert.equal(input.dispatch.length,12,'independent dispatch floor');
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i)),output={routes:[],dispatch:[]};
for(const c of input.routes){
 const s=c.scenario,trace=[],error=new TypeError('routing failure');let original,receiver,argReads=0,currentChannel;
 const effect=(name,j,key)=>{if(j){assert.equal(j,original,'source original parsed identity');assert.equal(j.payload,17,'source decoded payload');}trace.push([name,key??null]);if(c.fail===name)throw error;if(c.flip&&name==='track')receiver.isDisposed=true;};
 const raw=s===24?{includes:n=>n==='login',toString:()=> 'false'}:c.raw;
 const jsonParse=data=>{effect('parse');const j=parser.exports.jsonParse(data);original=j;if(j&&typeof j==='object'){
  const code=j.code;Object.defineProperty(j,'code',{get(){effect('code');return code;}});
  const msg=j.msg;Object.defineProperty(j,'msg',{get(){if(c.fail==='msg')throw error;return msg;}});
  const arg=j.arg;if(arg){for(const field of ['channel','instId','instType']){const value=arg[field];let n=0;Object.defineProperty(arg,field,{get(){n++;const result=s===25&&field==='channel'&&n===2?5:value;if(field==='channel')currentChannel=result;return result;}});}}
  Object.defineProperty(j,'arg',{get(){argReads++;return arg;}});
 }return j;};
 const module={exports:{}},context=vm.createContext({jsonParse});
 const code=ts.transpileModule((c.kind==='business'?constants:'')+'\nexport class Selected {'+methods[c.kind]+'}\n'+(c.kind==='business'?'exports.table=TIMEFRAME_BY_CANDLE_CHANNEL;':''),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 vm.runInContext('(function(module,exports){'+code+'\n})',context)(module,module.exports);
 if(s===24){context.getBooleanCode=()=>{effect('code');return 0;};vm.runInContext('Object.defineProperty(Boolean.prototype,"code",{get:getBooleanCode})',context);}
 receiver=new module.exports.Selected();receiver.isDisposed=c.disposed;receiver.trackLastReceived=()=>effect('track');receiver.directFallback={onMessage:()=>effect('fallback')};receiver.emitWsDiagnosticReceive=data=>{assert.equal(data,raw);effect('diagnostic');};receiver.handlePongEvent=()=>effect('pong');receiver.subscribe=()=>effect('subscribe');receiver.parent={emitAndLogError:msg=>{assert.equal(msg,'bad');effect('emit-error',original);}};
 for(const [method,name]of [['handleOrderTopic','order'],['handlePositionTopic','position'],['handleAccountTopic','account']])receiver[method]=j=>effect(name,j);
 receiver.recordMessageHandlerStats=k=>effect('stats',undefined,units(k));
 receiver.dispatchMessageHandler=(key,j)=>{const rawKey=key===currentChannel;effect(rawKey?'dispatch-value':'dispatch',j,rawKey?(typeof key==='string'?{string:units(key)}:{number:key}):units(key));};
 if(c.kind==='business'){
  const set=module.exports.ORDER_CHANNELS,has=set.has;set.has=function(channel){effect('membership');return has.call(this,channel);};if(s===23)set.add('custom-order');if(s===25)set.add('5');
  const table=module.exports.table,proto=Object.create(null);Object.defineProperty(proto,'__proto__',{value:{toString(){effect('format');if(s===21)throw error;return '[object Object]';}}});Object.setPrototypeOf(table,proto);
  for(const key of ['candle1m','__proto__','positions','account','5']){const value=table[key];Object.defineProperty(table,key,{get(){effect('timeframe');return value;}});}
 }
 let failed=false;try{receiver.onMessage({data:raw});}catch(e){assert.equal(e,error,'source original error identity');failed=true;}
 output.routes.push({trace,error:failed,argReads});
}
const dispatch=member(files.business,'OKXBusinessWebsocket','dispatchMessageHandler');
for(const scenario of input.dispatch){const trace=[],error=new TypeError('routing failure'),module={exports:{}},code=ts.transpileModule('export class Selected {'+dispatch+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;vm.runInThisContext('(function(module,exports){'+code+'\n})')(module,module.exports);const receiver=new module.exports.Selected(),json={payload:17},handler=scenario===11?5:'\ud800K';
 const fail=({3:'lookup',4:'length',5:'stats',6:'for-of',7:'cb:1'})[scenario]??'',call=name=>{trace.push([name,null]);if(name===fail)throw error;};
 let iterating=false,array;
 const cb=id=>j=>{assert.equal(j,json,'source message identity');assert.equal(j.payload,17);call('cb:'+id);if(scenario===9&&id===1)array.push(cb(3));if(scenario===10&&id===1)array.length=1;};array=[cb(1),cb(2)];if(scenario===1)array.length=0;
 const original=new Proxy(array,{get(target,key,receiver){if(key==='length'&&!iterating)call('length');if(key===Symbol.iterator){call('for-of');iterating=true;}return Reflect.get(target,key,receiver);}});let current=original;
 receiver.messageHandlers=new Proxy({}, {get(_,key){assert.equal(key,String(handler));call('lookup');return scenario===0?undefined:current;}});receiver.recordMessageHandlerStats=h=>{assert.equal(h,handler,'source original handler argument');call('stats');if(scenario===8)current=[cb(9)];};
 let failed=false;try{receiver.dispatchMessageHandler(handler,json);}catch(e){assert.equal(e,error);failed=true;}output.dispatch.push({trace,error:failed});
}
process.stdout.write(JSON.stringify(output));
