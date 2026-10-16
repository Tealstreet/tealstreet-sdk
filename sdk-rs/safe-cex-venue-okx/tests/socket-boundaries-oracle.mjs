import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const input=JSON.parse(fs.readFileSync(0,'utf8'));
assert.equal(input.wrappers.length,input.venue==='okx'?6:20);
assert.equal(input.disposals.length,input.venue==='okx'?5:14);
const sources={okx:'okx/okx.exchange.ts',futures:'kucoin/kucoin.exchange.ts',spot:'kucoin/kucoinspot.exchange.ts'};
function method(kind,name){
 const url=new URL('../../../packages/safe-cex/src/exchanges/'+sources[kind],import.meta.url);
 const source=ts.createSourceFile(url.pathname,fs.readFileSync(url,'utf8'),ts.ScriptTarget.Latest,true);
 const cls=source.statements.find(ts.isClassDeclaration),m=cls.members.find(n=>n.name?.getText(source)===name);
 assert.ok(m,'maintained '+kind+'/'+name);
 const module={exports:{}};
 vm.runInNewContext(ts.transpileModule('export class Selected extends Parent {'+m.getText(source)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,{module,exports:module.exports,Parent:class{dispose(){this.superDispose();}}});
 return new module.exports.Selected();
}
function failure(){return Object.assign(new TypeError('socket boundary failure'),{code:'SOCKET',ids:[9,2]});}
function checkError(e,original){assert.equal(e,original);assert.equal(e.name,'TypeError');assert.equal(e.message,'socket boundary failure');assert.equal(e.code,'SOCKET');assert.deepEqual(e.ids,[9,2]);}
function wrapper(c){
 const {kind,method:name,scenario}=c,events=[],err=failure(),receiver=method(kind,name),arg={tag:'arg',units:[0xd800,65]},callback=()=>{},result={tag:'result'};
 let providerReads=0;
 const external=['provider-true','provider-change'].includes(scenario);
 const owners={};for(const owner of ['publicWebsocket','businessWebsocket','externalPublicDataProvider']){
  const host={tag:owner};owners[owner]=host;
  for(const member of ['listenOHLCV','listenOrderBook','subscribeCandle','subscribeOrderbook'])Object.defineProperty(host,member,{get(){events.push('capture:'+owner+'.'+member);if(scenario==='capture-error')throw err;return function(a,b){assert.equal(this,host);assert.equal(a,arg);assert.equal(b,callback);events.push('invoke:'+owner+'.'+member);if(scenario==='invoke-error')throw err;return result;};}});
  if(owner!=='externalPublicDataProvider')Object.defineProperty(receiver,owner,{get(){events.push('read:'+owner);return host;}});
 }
 Object.defineProperty(receiver,'externalPublicDataProvider',{get(){events.push('read:provider');if(scenario==='provider-error')throw err;providerReads++;if(scenario==='provider-change'&&providerReads===2){const changed={};const member=name==='listenOHLCV'?'subscribeCandle':'subscribeOrderbook';Object.defineProperty(changed,member,{get(){events.push('capture:changed.'+member);return function(a,b){assert.equal(this,changed);assert.equal(a,arg);assert.equal(b,callback);events.push('invoke:changed.'+member);return result;};}});return changed;}return external?owners.externalPublicDataProvider:null;}});
 let throws=false;try{assert.equal(receiver[name](arg,callback),result);}catch(e){checkError(e,err);throws=true;}
 return {...c,events,throw:throws};
}
function disposal(c){
 const {kind,scenario}=c,receiver=method(kind,'dispose'),events=[],err=failure(),retained=[];
 function action(name){events.push(name);if(scenario===name+'-error')throw err;}
 receiver.superDispose=()=>action('super');
 for(const owner of ['publicWebsocket','privateWebsocket','businessWebsocket'])Object.defineProperty(receiver,owner,{get(){events.push('read:'+owner);const host={};Object.defineProperty(host,'dispose',{get(){events.push('capture:'+owner);return function(){assert.equal(this,host);action(owner);};}});return host;}});
 let snapshots=[{tag:'A'},{tag:'B'}];
 for(const socket of snapshots)socket.dispose=function(){assert.equal(this,socket);action('snapshot:'+socket.tag);if(scenario==='snapshots-error')throw err;};
 if(scenario==='empty-snapshots')snapshots=[];
 if(scenario==='retained-snapshot')snapshots.forEach=callback=>{events.push('forEach');retained.push(callback);};
 else if(scenario==='reentrant-snapshot')snapshots.forEach=callback=>{events.push('forEach');const inner={dispose(){action('snapshot:INNER');}};callback({dispose(){action('snapshot:OUTER');callback(inner);}});};
 else {const original=snapshots;Object.defineProperty(snapshots,'forEach',{get(){events.push('forEach');if(scenario==='forEach-error')throw err;return function(callback){assert.equal(this,original);return Array.prototype.forEach.call(original,callback);};}});}
 Object.defineProperty(receiver,'snapshotWebsockets',{get(){events.push('read:snapshots');return snapshots;},set(v){events.push('reset');assert.deepEqual(Array.from(v),[]);if(scenario==='reset-error')throw err;snapshots=v;}});
 let throws=false;try{assert.equal(receiver.dispose(),undefined);if(kind==='futures'&&!['snapshots-error','reset-error'].includes(scenario))assert.equal(snapshots.length,0);for(const callback of retained){callback({dispose(){action('snapshot:LATE');}});}}catch(e){checkError(e,err);throws=true;}
 return {...c,events,throw:throws};
}
process.stdout.write(JSON.stringify({venue:input.venue,wrappers:input.wrappers.map(wrapper),disposals:input.disposals.map(disposal)}));
